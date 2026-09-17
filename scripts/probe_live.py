"""Live probe of Deadlock's memory to locate the local pawn global.

Reads the running game externally (ReadProcessMemory only — never writes):
 1. resolves client.dll's base in the live process
 2. verifies the pattern-scanned entity-system global and walks the entity list
 3. groups entities by vtable to isolate hero-pawn candidates
 4. scans .data for the singleton slot that points at a hero pawn — that slot
    is the current C_CitadelPlayerPawn global offset
 5. validates pawn/ability field offsets against the live pawn
"""

import ctypes
import ctypes.wintypes as wt
import struct
import sys

k32 = ctypes.WinDLL("kernel32", use_last_error=True)
psapi = ctypes.WinDLL("psapi", use_last_error=True)

PROCESS_VM_READ = 0x0010
PROCESS_QUERY_INFORMATION = 0x0400

k32.OpenProcess.restype = ctypes.c_void_p
k32.OpenProcess.argtypes = [wt.DWORD, wt.BOOL, wt.DWORD]
k32.ReadProcessMemory.restype = wt.BOOL
k32.ReadProcessMemory.argtypes = [
    ctypes.c_void_p,
    ctypes.c_void_p,
    ctypes.c_void_p,
    ctypes.c_size_t,
    ctypes.POINTER(ctypes.c_size_t),
]
k32.CloseHandle.argtypes = [ctypes.c_void_p]
psapi.EnumProcessModulesEx.restype = wt.BOOL
psapi.EnumProcessModulesEx.argtypes = [
    ctypes.c_void_p,
    ctypes.POINTER(ctypes.c_void_p),
    wt.DWORD,
    ctypes.POINTER(wt.DWORD),
    wt.DWORD,
]
psapi.GetModuleFileNameExW.restype = wt.DWORD
psapi.GetModuleFileNameExW.argtypes = [
    ctypes.c_void_p,
    ctypes.c_void_p,
    ctypes.c_wchar_p,
    wt.DWORD,
]

process = None


def open_game():
    global process
    pid = int(sys.argv[1]) if len(sys.argv) > 1 else None
    if pid is None:
        import subprocess

        out = subprocess.run(
            ["tasklist", "/FI", "IMAGENAME eq deadlock.exe", "/FO", "CSV"],
            capture_output=True,
            text=True,
        ).stdout
        for line in out.splitlines()[1:]:
            pid = int(line.split('","')[1])
            break
    if not pid:
        raise SystemExit("deadlock.exe not running")
    process = k32.OpenProcess(PROCESS_VM_READ | PROCESS_QUERY_INFORMATION, False, pid)
    if not process:
        raise SystemExit(f"cannot open pid {pid}: err={ctypes.get_last_error()}")
    print(f"attached to deadlock.exe pid {pid}")


def read(address, size):
    buf = ctypes.create_string_buffer(size)
    got = ctypes.c_size_t(0)
    ok = k32.ReadProcessMemory(process, ctypes.c_void_p(address), buf, size, ctypes.byref(got))
    if not ok or got.value != size:
        return None
    return buf.raw


def read_u64(address):
    raw = read(address, 8)
    return struct.unpack("<Q", raw)[0] if raw else None


def read_u32(address):
    raw = read(address, 4)
    return struct.unpack("<I", raw)[0] if raw else None


def client_base():
    LIST_MODULES_ALL = 0x03
    needed = wt.DWORD(0)
    psapi.EnumProcessModulesEx(process, None, 0, ctypes.byref(needed), LIST_MODULES_ALL)
    count = needed.value // ctypes.sizeof(ctypes.c_void_p)
    modules = (ctypes.c_void_p * count)()
    if not psapi.EnumProcessModulesEx(
        process, modules, needed.value, ctypes.byref(needed), LIST_MODULES_ALL
    ):
        raise SystemExit("EnumProcessModulesEx failed")
    for module in modules:
        name = ctypes.create_unicode_buffer(512)
        psapi.GetModuleFileNameExW(process, module, name, 512)
        if name.value.lower().endswith("\\client.dll"):
            return ctypes.cast(module, ctypes.c_void_p).value
    raise SystemExit("client.dll not loaded")


def main():
    open_game()
    base = client_base()
    print(f"client.dll base: 0x{base:X}")

    ENTITY_SYSTEM_RVA = 0x391EDA8  # from patterns.json runtime scan (dwEntityList/dwGameEntitySystem agree)

    esys_ptr_addr = base + ENTITY_SYSTEM_RVA
    esys = read_u64(esys_ptr_addr)
    print(f"entity system global @0x{ENTITY_SYSTEM_RVA:X} -> 0x{esys:X}" if esys else "entity system null")

    # ---- walk the entity list: chunk array at +0x10, 512 slots/chunk, 0x78 stride
    CHUNK_ARRAY = 0x10
    CHUNK_SIZE = 512
    STRIDE = 0x70
    MAX_CHUNKS = 8

    entities = {}
    for chunk_idx in range(MAX_CHUNKS):
        chunk = read_u64(esys + CHUNK_ARRAY + 8 * chunk_idx)
        if not chunk:
            break
        blob = read(chunk, CHUNK_SIZE * STRIDE)
        if blob is None:
            continue
        for slot in range(CHUNK_SIZE):
            ent = struct.unpack_from("<Q", blob, slot * STRIDE)[0]
            if ent:
                entities[chunk_idx * CHUNK_SIZE + slot] = ent
    print(f"entities resolved: {len(entities)}")

    # ---- group by vtable
    vtables = {}
    for idx, ent in entities.items():
        vt = read_u64(ent)
        if vt:
            vtables.setdefault(vt, []).append(idx)

    hero_groups = [(vt, idxs) for vt, idxs in vtables.items() if 3 <= len(idxs) <= 40]
    print(f"vtable groups of size 3..40 (hero candidates): {len(hero_groups)}")

    # health/lifestate per candidate hero entity (schema field offsets)
    HEALTH, MAX_HEALTH, LIFESTATE = 0x2D0, 0x2D4, 0x2D8
    heroes = []
    for vt, idxs in sorted(hero_groups, key=lambda pair: -len(pair[1])):
        sample = []
        for idx in idxs[:6]:
            ent = entities[idx]
            raw = read(ent + HEALTH, 8)
            if raw is None:
                continue
            hp, maxhp = struct.unpack("<ii", raw)
            ls = read_u32(ent + LIFESTATE) or 255
            sample.append((idx, hp, maxhp, ls & 0xFF))
        plausible = [s for s in sample if 0 <= s[1] <= 5000 and 100 <= s[2] <= 10000 and s[3] <= 8]
        if len(plausible) >= max(1, len(sample) // 2):
            heroes.append((vt, idxs, plausible))
            print(f"hero pawn vtable 0x{vt:X}: {len(idxs)} entities; sample {sample[:3]}")

    if not heroes:
        raise SystemExit("no hero-pawn vtable group found — entity layout or field offsets stale")

    # ---- scan .data for the singleton pointing at one of these hero entities
    DATA_RVA, DATA_SIZE = 0x2D8C000, 0xC059CC
    blob = read(base + DATA_RVA, DATA_SIZE)
    if blob is None:
        raise SystemExit("could not read .data section")
    hero_set = {}
    for vt, idxs, _ in heroes:
        for idx in idxs:
            hero_set[entities[idx]] = idx

    print("scanning .data for pawn globals...")
    hits = {}
    for off in range(0, DATA_SIZE - 8, 4):
        val = struct.unpack_from("<Q", blob, off)[0]
        if val in hero_set:
            hits.setdefault(val, []).append(DATA_RVA + off)

    print()
    found_globals = []
    for ent, slots in sorted(hits.items(), key=lambda pair: -len(pair[1])):
        raw = read(ent + HEALTH, 8)
        hp, maxhp = struct.unpack("<ii", raw) if raw else (None, None)
        ls = (read_u32(ent + LIFESTATE) or 255) & 0xFF
        slots_str = ", ".join(f"0x{s:X}" for s in slots)
        print(f"pawn 0x{ent:X} (idx={hero_set[ent]} hp={hp}/{maxhp} life_state={ls}): slot(s) {slots_str}")
        found_globals.extend(slots)

    # ---- validate ability fields on the first found pawn
    if found_globals:
        ent = next(e for e, s in hits.items() if found_globals[0] in s)
        print()
        ABILITIES = 0x1A58
        vec = read(ent + ABILITIES, 16)
        if vec:
            count, data_ptr = struct.unpack("<IQ", vec)
            print(f"abilities vector @+0x{ABILITIES:X}: count={count} data=0x{data_ptr:X}")
            if 0 < count < 64 and data_ptr:
                for i in range(min(count, 20)):
                    handle = read_u32(data_ptr + 4 * i)
                    if handle is None:
                        continue
                    index = handle & 0x7FFF
                    chunk_idx, slot = index // CHUNK_SIZE, index % CHUNK_SIZE
                    chunk = read_u64(esys + CHUNK_ARRAY + 8 * chunk_idx)
                    if not chunk:
                        continue
                    ability = read_u64(chunk + slot * STRIDE)
                    if not ability:
                        continue
                    slot_field = read(ability + 0x778, 2)
                    cend_raw = read(ability + 0x768, 4)
                    charges = read(ability + 0x780, 4)
                    if slot_field and cend_raw and charges:
                        s = struct.unpack("<H", slot_field)[0]
                        cend = struct.unpack("<f", cend_raw)[0]
                        ch = struct.unpack("<i", charges)[0]
                        print(f"  ability[{i}]: handle={handle:#x} slot={s} cooldown_end={cend:.2f} charges={ch}")

    print()
    if found_globals:
        print("=> C_CitadelPlayerPawn global candidates (relative to client.dll):")
        for s in sorted(set(found_globals)):
            print(f"   0x{s:X}")


k32.CloseHandle.restype = wt.BOOL

if __name__ == "__main__":
    main()
