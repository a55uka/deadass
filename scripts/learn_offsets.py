"""Learn current Deadlock client offsets from the live game.

Walks the entity list (layout already confirmed live), isolates the hero-pawn
vtable group by population (6v6 match => ~12 heroes), then:
  * locates the current m_iHealth/m_iMaxHealth/m_lifeState offsets by
    cross-entity consistency
  * locates the current abilities vector offset
  * scans .data for the singleton slot pointing at a hero entity => the
    C_CitadelPlayerPawn global offset
  * validates ability entity field offsets via the resolved ability handles
"""

import ctypes
import ctypes.wintypes as wt
import struct
import subprocess
import sys

k32 = ctypes.WinDLL("kernel32", use_last_error=True)
k32.OpenProcess.restype = ctypes.c_void_p
k32.OpenProcess.argtypes = [wt.DWORD, wt.BOOL, wt.DWORD]
k32.ReadProcessMemory.restype = wt.BOOL
k32.ReadProcessMemory.argtypes = [
    ctypes.c_void_p, ctypes.c_void_p, ctypes.c_void_p, ctypes.c_size_t,
    ctypes.POINTER(ctypes.c_size_t),
]

process = None
BASE = None


def attach():
    global process, BASE
    out = subprocess.run(
        ["tasklist", "/FI", "IMAGENAME eq deadlock.exe", "/FO", "CSV"],
        capture_output=True, text=True,
    ).stdout
    pid = None
    for line in out.splitlines()[1:]:
        pid = int(line.split('","')[1])
        break
    if not pid:
        raise SystemExit("deadlock.exe not running")
    process = k32.OpenProcess(0x0410, False, pid)
    if not process:
        raise SystemExit("cannot open process (run as admin?)")

    psapi = ctypes.WinDLL("psapi")
    psapi.EnumProcessModulesEx.restype = wt.BOOL
    psapi.EnumProcessModulesEx.argtypes = [ctypes.c_void_p, ctypes.POINTER(ctypes.c_void_p), wt.DWORD, ctypes.POINTER(wt.DWORD), wt.DWORD]
    psapi.GetModuleFileNameExW.restype = wt.DWORD
    psapi.GetModuleFileNameExW.argtypes = [ctypes.c_void_p, ctypes.c_void_p, ctypes.c_wchar_p, wt.DWORD]
    needed = wt.DWORD(0)
    psapi.EnumProcessModulesEx(process, None, 0, ctypes.byref(needed), 0x03)
    mods = (ctypes.c_void_p * (needed.value // 8))()
    psapi.EnumProcessModulesEx(process, mods, needed.value, ctypes.byref(needed), 0x03)
    for mod in mods:
        name = ctypes.create_unicode_buffer(512)
        psapi.GetModuleFileNameExW(process, mod, name, 512)
        if name.value.lower().endswith("\\client.dll"):
            BASE = ctypes.cast(mod, ctypes.c_void_p).value
            break
    print(f"pid={pid} client.dll base=0x{BASE:X}")


def read(address, size):
    buf = ctypes.create_string_buffer(size)
    got = ctypes.c_size_t(0)
    if not k32.ReadProcessMemory(process, ctypes.c_void_p(address), buf, size, ctypes.byref(got)):
        return None
    return buf.raw[: got.value]


def u64(addr):
    raw = read(addr, 8)
    return struct.unpack("<Q", raw)[0] if raw else None


def u32(addr):
    raw = read(addr, 4)
    return struct.unpack("<I", raw)[0] if raw else None


def walk_entities(esys):
    CHUNK_ARRAY, CHUNK_SIZE, STRIDE, MAX_CHUNKS = 0x10, 512, 0x70, 8
    entities = {}
    for ci in range(MAX_CHUNKS):
        chunk = u64(esys + CHUNK_ARRAY + 8 * ci)
        if not chunk:
            break
        blob = read(chunk, CHUNK_SIZE * STRIDE)
        if blob is None:
            continue
        for slot in range(CHUNK_SIZE):
            ent = struct.unpack_from("<Q", blob, slot * STRIDE)[0]
            if ent:
                entities[ci * CHUNK_SIZE + slot] = ent
    return entities


def main():
    attach()
    ENTITY_SYSTEM_RVA = 0x391EDA8
    esys = u64(BASE + ENTITY_SYSTEM_RVA)
    if not esys:
        raise SystemExit("entity system global is null (not in a match?)")
    entities = walk_entities(esys)
    print(f"entity system 0x{esys:X}, entities: {len(entities)}")

    # ---- hero group by vtable population
    vtables = {}
    for idx, ent in entities.items():
        vt = u64(ent)
        if vt:
            vtables.setdefault(vt, []).append(idx)
    ordered = sorted(vtables.items(), key=lambda pair: -len(pair[1]))
    print("top vtable groups:", [(f"0x{vt:X}", len(idxs)) for vt, idxs in ordered[:6]])

    hero_vt, hero_idxs = next(
        ((vt, idxs) for vt, idxs in ordered if 8 <= len(idxs) <= 24), (None, None)
    )
    if not hero_idxs:
        raise SystemExit("no group with 8..24 entities (not in a 6v6 match?)")
    print(f"hero group: vtable 0x{hero_vt:X} with {len(hero_idxs)} entities")

    # ---- learn health / max_health / life_state offsets by consistency
    candidates = []
    for off in range(0x0, 0x2400, 4):
        ok = 0
        for idx in hero_idxs:
            ent = entities[idx]
            raw = read(ent + off, 8)
            if raw is None:
                continue
            a, b = struct.unpack("<ii", raw)
            if 0 <= a <= 6000 and 100 <= b <= 10000 and a <= b + 500:
                ok += 1
        if ok >= max(4, int(len(hero_idxs) * 0.6)):
            candidates.append((off, ok))
    print("health/max_health offset candidates:", [(f"0x{o:X}", n) for o, n in candidates])

    best = candidates[0][0] if candidates else None
    if best is None:
        raise SystemExit("no health offset found — heroes all dead?")
    ls_scores = {}
    for off in range(best - 8, best + 24):
        ok = 0
        for idx in hero_idxs:
            raw = read(entities[idx] + off, 1)
            if raw is not None and raw[0] <= 4:
                ok += 1
        ls_scores[off] = ok
    ls_off = max(ls_scores, key=ls_scores.get)
    print(f"life_state candidate: 0x{ls_off:X} (health at 0x{best:X})")

    sample = []
    for idx in list(hero_idxs)[:6]:
        hp, maxhp = struct.unpack("<ii", read(entities[idx] + best, 8))
        ls = read(entities[idx] + ls_off, 1)[0]
        sample.append((idx, hp, maxhp, ls))
    print("hero sample (idx, hp, maxhp, life):", sample)

    # ---- learn abilities vector offset
    ability_offsets = []
    for off in range(0x0, 0x2600, 4):
        ok = 0
        for idx in hero_idxs:
            ent = entities[idx]
            raw = read(ent + off, 12)
            if raw is None:
                continue
            count = struct.unpack_from("<I", raw)[0]
            ptr = struct.unpack_from("<Q", raw, 4)[0]
            if 8 <= count <= 48 and ptr > 0x10000:
                ok += 1
        if ok >= len(hero_idxs) - 1:
            ability_offsets.append(off)
    print("abilities vector offset candidates:", [f"0x{o:X}" for o in ability_offsets])
    ABILITIES = ability_offsets[0] if ability_offsets else None

    # ---- find the .data singleton pointing at a hero => local pawn global
    DATA_RVA, DATA_SIZE = 0x2D8C000, 0xC059CC
    blob = read(BASE + DATA_RVA, DATA_SIZE)
    hero_set = {entities[i]: i for i in hero_idxs}
    hits = {}
    for off in range(0, DATA_SIZE - 8, 4):
        val = struct.unpack_from("<Q", blob, off)[0]
        if val in hero_set:
            hits.setdefault(val, []).append(DATA_RVA + off)
    print()
    for ent, slots in hits.items():
        hp, maxhp = struct.unpack("<ii", read(ent + best, 8))
        ls = read(ent + ls_off, 1)[0]
        print(f"pawn 0x{ent:X} (idx={hero_set[ent]} hp={hp}/{maxhp} life={ls}) <- .data slot(s): "
              + ", ".join(f"0x{s:X}" for s in slots))

    # ---- validate ability entity fields through the handles
    if ABILITIES and hits:
        ent = next(iter(hits))
        count = u32(ent + ABILITIES)
        ptr = u64(ent + ABILITIES + 4 + 4)
        print(f"\nabilities of pawn 0x{ent:X}: count={count}")
        CHUNK_SIZE, STRIDE, CHUNK_ARRAY = 512, 0x70, 0x10
        for i in range(min(count or 0, 20)):
            handle = u32(ptr + 4 * i)
            if not handle:
                continue
            index = handle & 0x7FFF
            chunk = u64(esys + CHUNK_ARRAY + 8 * (index // CHUNK_SIZE))
            if not chunk:
                continue
            ability = u64(chunk + (index % CHUNK_SIZE) * STRIDE)
            if not ability:
                continue
            # dump the plausible ability-state window for manual inspection
            win = read(ability + 0x700, 0x100)
            if win is None:
                continue
            slot_vals = [(0x700 + o, struct.unpack_from("<H", win, o)[0])
                         for o in range(0, 0x100, 2)
                         if struct.unpack_from("<H", win, o)[0] <= 5]
            float_vals = [(0x700 + o, round(struct.unpack_from("<f", win, o)[0], 2))
                          for o in range(0, 0xFC, 4)
                          if 1.0 < abs(struct.unpack_from("<f", win, o)[0]) < 300.0]
            print(f"  ability[{i}] 0x{ability:X}: small_u16={slot_vals[:6]} floats={float_vals[:6]}")


if __name__ == "__main__":
    main()
