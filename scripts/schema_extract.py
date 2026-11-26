"""Extract current field offsets from client.dll's recv-table descriptors —
no game code is called, only memory reads — and emit deadass-offsets.toml.

Descriptors live in client.dll's data: entry stride 0x20 =
{+0x00 name char*, +0x08 type ptr, +0x10 {u32 offset, u32 flags}, +0x18 ptr}.
Duplicated field names are disambiguated by the field names that sit in the
same table.

The two globals (local pawn, entity system) are auto-discovered by memory
signature — no dezlock dump needed:
  entity system: a client.dll data pointer to a heap object whose inline
    chunk array (+0x10) holds chunk pointers to entity slots with client
    vtables.
  local pawn: a client.dll data pointer whose value is an entity in the
    entity list whose controller handle resolves to a controller whose
    m_hPawn points back at the same entity. Needs a live match; in the
    menu the last known value is kept.

Usage (game running, from the deadass repo root):
    python scripts/schema_extract.py > deadass-offsets.toml
Works on Windows and on Linux under Proton (reads /proc/<pid>/mem; needs
kernel.yama.ptrace_scope=0 or root). --pid picks the process explicitly.
Fields only, no game running (globals keep the KNOWN_* fallbacks):
    python scripts/schema_extract.py --static <Deadlock>/game/citadel/bin/win64/client.dll
Field candidates and discovery notes go to stderr.
"""

import argparse, bisect, os, struct, subprocess, sys

# Fallbacks used only when auto-discovery cannot run (e.g. pawn discovery
# needs a live match). Update after a fresh dump if you skip discovery.
KNOWN_PAWN_GLOBAL = 0x3496c48
KNOWN_ES_GLOBAL = 0x3eb4b78

parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
parser.add_argument("--static", metavar="CLIENT_DLL",
                    help="read field offsets from a client.dll on disk instead of the live "
                         "game; globals cannot be discovered and keep the KNOWN_* values")
parser.add_argument("--pid", type=int, help="attach to this process id (Linux/Proton)")
ARGS = parser.parse_args()


# ---- process backends: each sets PID, BASE, MODULE_RANGES, read(), committed() ----

def windows_backend():
    import ctypes, ctypes.wintypes as wt
    k32 = ctypes.WinDLL("kernel32", use_last_error=True)
    k32.OpenProcess.restype = ctypes.c_void_p
    k32.OpenProcess.argtypes = [wt.DWORD, wt.BOOL, wt.DWORD]
    k32.ReadProcessMemory.restype = wt.BOOL
    k32.ReadProcessMemory.argtypes = [ctypes.c_void_p, ctypes.c_void_p, ctypes.c_void_p, ctypes.c_size_t, ctypes.POINTER(ctypes.c_size_t)]
    k32.VirtualQueryEx.restype = ctypes.c_size_t
    k32.VirtualQueryEx.argtypes = [ctypes.c_void_p, ctypes.c_void_p, ctypes.c_void_p, ctypes.c_size_t]
    psapi = ctypes.WinDLL("psapi")
    psapi.EnumProcessModulesEx.restype = wt.BOOL
    psapi.EnumProcessModulesEx.argtypes = [ctypes.c_void_p, ctypes.POINTER(ctypes.c_void_p), wt.DWORD, ctypes.POINTER(wt.DWORD), wt.DWORD]
    psapi.GetModuleFileNameExW.restype = wt.DWORD
    psapi.GetModuleFileNameExW.argtypes = [ctypes.c_void_p, ctypes.c_void_p, ctypes.c_wchar_p, wt.DWORD]

    if ARGS.pid:
        pids = [ARGS.pid]
    else:
        out = subprocess.run(["tasklist", "/FI", "IMAGENAME eq deadlock.exe", "/FO", "CSV"], capture_output=True, text=True).stdout
        pids = [int(line.split('","')[1]) for line in out.splitlines() if "deadlock" in line.lower()]
    if not pids:
        sys.exit("deadlock.exe is not running")

    # several deadlock.exe processes can exist (launcher/loader + the game);
    # attach to the one that actually has client.dll mapped
    process = pid = base_found = None
    bases = []
    for candidate in pids:
        handle = k32.OpenProcess(0x0410, False, candidate)
        if not handle:
            continue
        needed = wt.DWORD(0)
        psapi.EnumProcessModulesEx(handle, None, 0, ctypes.byref(needed), 0x03)
        mods = (ctypes.c_void_p * (needed.value // 8))()
        psapi.EnumProcessModulesEx(handle, mods, needed.value, ctypes.byref(needed), 0x03)
        these = []
        base = None
        for mod in mods:
            name = ctypes.create_unicode_buffer(512)
            psapi.GetModuleFileNameExW(handle, mod, name, 512)
            these.append(ctypes.cast(mod, ctypes.c_void_p).value)
            if name.value.lower().endswith("\\client.dll"):
                base = ctypes.cast(mod, ctypes.c_void_p).value
        if base is not None:
            process, pid, base_found, bases = handle, candidate, base, these
            break
    if process is None:
        sys.exit(f"no deadlock.exe process (of {len(pids)}) has client.dll loaded yet")

    def read(addr, size):
        buf = ctypes.create_string_buffer(size)
        got = ctypes.c_size_t(0)
        if not addr or not k32.ReadProcessMemory(process, ctypes.c_void_p(addr), buf, size, ctypes.byref(got)):
            return None
        return buf.raw[:got.value]

    class MBI(ctypes.Structure):
        _fields_ = [("BaseAddress", ctypes.c_void_p), ("AllocationBase", ctypes.c_void_p),
                    ("AllocationProtect", wt.DWORD), ("PartitionId", wt.DWORD),
                    ("RegionSize", ctypes.c_size_t), ("State", wt.DWORD),
                    ("Protect", wt.DWORD), ("Type", wt.DWORD)]

    def committed(addr):
        mbi = MBI()
        return bool(k32.VirtualQueryEx(process, ctypes.c_void_p(addr), ctypes.byref(mbi), ctypes.sizeof(mbi))) and mbi.State == 0x1000

    # generous 128MB span per module; exact PE sizes are not needed here
    ranges = [(b, b + 0x8000000) for b in bases]
    return pid, base_found, ranges, read, committed


def linux_backend():
    """Deadlock under Proton/Wine: the game is a normal Linux process whose
    /proc/<pid>/maps lists client.dll; memory is read through /proc/<pid>/mem
    (needs kernel.yama.ptrace_scope=0, or run as root)."""
    def maps_of(pid):
        regions = []
        try:
            with open(f"/proc/{pid}/maps") as handle:
                for line in handle:
                    parts = line.split(maxsplit=5)
                    lo, hi = (int(x, 16) for x in parts[0].split("-"))
                    path = parts[5].strip() if len(parts) > 5 else ""
                    regions.append((lo, hi, parts[1], path))
        except OSError:
            return None
        return regions

    if ARGS.pid:
        pids = [ARGS.pid]
    else:
        pids = []
        for entry in os.listdir("/proc"):
            if not entry.isdigit():
                continue
            try:
                with open(f"/proc/{entry}/cmdline", "rb") as handle:
                    cmdline = handle.read().lower()
            except OSError:
                continue
            if b"deadlock.exe" in cmdline:
                pids.append(int(entry))
    if not pids:
        sys.exit("deadlock.exe is not running (no process with deadlock.exe in its cmdline)")

    pid = regions = client_path = None
    for candidate in pids:
        maps = maps_of(candidate)
        if not maps:
            continue
        hit = next((path for _, _, _, path in maps if path.lower().endswith("/client.dll")), None)
        if hit:
            pid, regions, client_path = candidate, maps, hit
            break
    if pid is None:
        sys.exit(f"no deadlock.exe process (of {len(pids)}) has client.dll mapped yet")

    try:
        mem = open(f"/proc/{pid}/mem", "rb", buffering=0)
    except PermissionError:
        sys.exit(f"cannot open /proc/{pid}/mem: set kernel.yama.ptrace_scope=0 "
                 "(sudo sysctl kernel.yama.ptrace_scope=0) or run as root")

    def read(addr, size):
        if not addr:
            return None
        try:
            data = os.pread(mem.fileno(), size, addr)
        except OSError:
            return None
        return data or None

    starts = [lo for lo, _, _, _ in regions]

    def committed(addr):
        i = bisect.bisect_right(starts, addr) - 1
        return i >= 0 and addr < regions[i][1] and regions[i][2][0] == "r"

    # PE modules: lowest mapping of each .dll/.exe is its image base; the
    # extent comes from SizeOfImage in that image's own header
    module_bases = {}
    for lo, _, _, path in regions:
        if path.lower().endswith((".dll", ".exe")):
            module_bases[path] = min(lo, module_bases.get(path, lo))
    ranges = []
    for path, base in module_bases.items():
        head = read(base, 0x400)
        if head and head[:2] == b"MZ":
            lfanew = struct.unpack_from("<I", head, 0x3C)[0]
            if lfanew + 0x54 <= len(head):
                ranges.append((base, base + struct.unpack_from("<I", head, lfanew + 0x50)[0]))
                continue
        ranges.append((base, base + 0x8000000))

    base = module_bases[client_path]

    # read() the whole image in one go fails if any page inside is
    # unreadable; wrap it so a big read falls back to page-sized pieces
    raw_read = read

    def read_resilient(addr, size):
        data = raw_read(addr, size)
        if data is not None and len(data) == size or size <= 0x1000:
            return data
        out = bytearray()
        pos = addr
        while pos < addr + size:
            step = min(0x100000, addr + size - pos)
            chunk = raw_read(pos, step)
            if chunk is None or len(chunk) < step:
                pieces = bytearray()
                for page in range(pos, pos + step, 0x1000):
                    got = raw_read(page, min(0x1000, pos + step - page))
                    pieces += got if got else b"\x00" * min(0x1000, pos + step - page)
                chunk = bytes(pieces)
            out += chunk
            pos += step
        return bytes(out)

    return pid, base, ranges, read_resilient, committed


def static_backend(path):
    """Lay a client.dll from disk out at its preferred ImageBase. The
    on-disk pointers already assume that base (relocations are not needed),
    so the recv-table descriptor scan works unchanged."""
    raw = open(path, "rb").read()
    lfanew = struct.unpack_from("<I", raw, 0x3C)[0]
    opt_size = struct.unpack_from("<H", raw, lfanew + 0x14)[0]
    count = struct.unpack_from("<H", raw, lfanew + 6)[0]
    image_base = struct.unpack_from("<Q", raw, lfanew + 0x18 + 0x18)[0]
    size_of_image = struct.unpack_from("<I", raw, lfanew + 0x18 + 0x38)[0]
    headers = struct.unpack_from("<I", raw, lfanew + 0x18 + 0x3C)[0]
    mapped = bytearray(size_of_image)
    mapped[:headers] = raw[:headers]
    first = lfanew + 0x18 + opt_size
    for i in range(count):
        header = first + i * 0x28
        vsize, vaddr, rsize, rptr = struct.unpack_from("<IIII", raw, header + 8)
        length = min(rsize, vsize) if vsize else rsize
        mapped[vaddr:vaddr + length] = raw[rptr:rptr + length]
    mapped = bytes(mapped)

    def read(addr, size):
        rel = addr - image_base
        if not addr or rel < 0 or rel >= len(mapped):
            return None
        return mapped[rel:rel + size]

    def committed(addr):
        return 0 <= addr - image_base < len(mapped)

    return None, image_base, [(image_base, image_base + size_of_image)], read, committed


if ARGS.static:
    PID, BASE, MODULE_RANGES, read, committed = static_backend(ARGS.static)
elif sys.platform == "win32":
    PID, BASE, MODULE_RANGES, read, committed = windows_backend()
else:
    PID, BASE, MODULE_RANGES, read, committed = linux_backend()
pid = PID

def live_u64(addr):
    raw = read(addr, 8)
    return struct.unpack("<Q", raw)[0] if raw and len(raw) == 8 else 0

def live_u32(addr):
    raw = read(addr, 4)
    return struct.unpack("<I", raw)[0] if raw and len(raw) == 4 else 0

def in_module(addr):
    return any(lo <= addr < hi for lo, hi in MODULE_RANGES)

# Windows ASLR puts heap above 1TB; Wine/Proton hands out low heap
# addresses as well, so accept any user-space address there
HEAP_MIN = 0x10000000000 if sys.platform == "win32" else 0x10000

def is_heap_ptr(value):
    return HEAP_MIN <= value < 0x7FFFFFFFFFFF and committed(value) and not in_module(value)

e_lfanew = struct.unpack("<I", read(BASE + 0x3C, 4))[0]
IMG_SIZE = struct.unpack("<I", read(BASE + e_lfanew + 0x50, 4))[0]
image = read(BASE, IMG_SIZE)
if not image:
    sys.exit("cannot read client.dll image")
if ARGS.static:
    print(f"# extracted from {ARGS.static} (static, ImageBase 0x{BASE:X})", file=sys.stderr)
else:
    print(f"# extracted from live deadlock.exe pid={pid}, client.dll+0x{BASE:X}", file=sys.stderr)

# ---- writable data sections of the image (global hunting ground) ----
def data_sections():
    count = struct.unpack_from("<H", image, e_lfanew + 6)[0]
    opt_size = struct.unpack_from("<H", image, e_lfanew + 0x14)[0]
    first = e_lfanew + 0x18 + opt_size
    sections = []
    for i in range(count):
        header = first + i * 0x28
        name = image[header:header + 8].rstrip(b"\x00").decode("ascii", "replace")
        vsize, vaddr = struct.unpack_from("<II", image, header + 8)
        chars = struct.unpack_from("<I", image, header + 0x24)[0]
        if chars & 0x80000000 and vaddr + vsize <= len(image):  # IMAGE_SCN_MEM_WRITE
            sections.append((name, vaddr, vsize))
    return sections

WRITABLE = data_sections()

def string_section():
    """read-only initialised data (.rdata): where field-name strings live"""
    count = struct.unpack_from("<H", image, e_lfanew + 6)[0]
    opt_size = struct.unpack_from("<H", image, e_lfanew + 0x14)[0]
    first = e_lfanew + 0x18 + opt_size
    for i in range(count):
        header = first + i * 0x28
        name = image[header:header + 8].rstrip(b"\x00")
        vsize, vaddr = struct.unpack_from("<II", image, header + 8)
        if name == b".rdata":
            return vaddr, vaddr + vsize
    return 0x2500000, 0x2C00000  # historical window

RDATA_LO, RDATA_HI = string_section()

# ---- global discovery ----
def entity_system_looks_like(value):
    """Candidate must (a) point to a heap object whose inline chunk array
    holds chunk pointers to client-vtable entities, and (b) those records
    back-pointer to their own chunk slot (record+0x10 == &chunk[slot]).
    Multiple systems share structure (a); (b) plus vtable diversity scoring
    separates the main CGameEntitySystem."""
    if not is_heap_ptr(value):
        return False
    chunk0 = live_u64(value + 0x10)
    if not is_heap_ptr(chunk0):
        return False
    good = 0
    for slot in range(4):
        entity = live_u64(chunk0 + slot * 0x70)
        if entity and committed(entity) and BASE <= live_u64(entity) < BASE + IMG_SIZE:
            good += 1
    return good >= 2

def backpointer_score(value):
    """(hits, misses) for record+0x10 == &chunk[slot] over chunk0 slots"""
    chunk0 = live_u64(value + 0x10)
    if not is_heap_ptr(chunk0):
        return 0, 0
    hits = misses = 0
    for slot in range(8):
        record = live_u64(chunk0 + slot * 0x70)
        if not record or not committed(record):
            continue
        if live_u64(record + 0x10) == chunk0 + slot * 0x70 and \
                BASE <= live_u64(record) < BASE + IMG_SIZE:
            hits += 1
        else:
            misses += 1
    return hits, misses

def vtable_diversity(value):
    """distinct client vtables across the first chunks — the main system
    holds the diverse world entities"""
    seen = set()
    for chunk_i in range(8):
        chunk = live_u64(value + 0x10 + chunk_i * 8)
        if not is_heap_ptr(chunk):
            continue
        for slot in range(512):
            record = live_u64(chunk + slot * 0x70)
            if record and committed(record):
                vt = live_u64(record)
                if BASE <= vt < BASE + IMG_SIZE:
                    seen.add(vt)
    return len(seen)

def discover_entity_system():
    candidates = []
    for name, start, size in WRITABLE:
        for off in range(start, start + size - 8, 8):
            value = struct.unpack_from("<Q", image, off)[0]
            if HEAP_MIN <= value < 0x7FFFFFFFFFFF and entity_system_looks_like(value):
                candidates.append((off, value))
    verified = []
    for off, value in candidates:
        hits, misses = backpointer_score(value)
        if hits >= 2 and misses == 0:
            verified.append((off, value))
    # rank by vtable diversity: the main entity system holds the most
    # varied world entities
    ranked = sorted(verified, key=lambda c: -vtable_diversity(c[1]))
    for off, value in ranked[1:]:
        print(f"# note: additional entity-system candidate client+0x{off:X} "
              f"(diversity {vtable_diversity(value)}) — main system picked by vtable diversity",
              file=sys.stderr)
    return ranked

def resolve_entity(handle, es_value):
    index = handle & 0x7FFF
    chunk = live_u64(es_value + 0x10 + (index >> 9) * 8)
    if not is_heap_ptr(chunk):
        return 0
    return live_u64(chunk + (index & 511) * 0x70)

def vtable_of(entity):
    value = live_u64(entity)
    return value if BASE <= value < BASE + IMG_SIZE else 0

def looks_like_pawn(value, es_value, health_off, max_health_off,
                    controller_off, pawn_handle_off):
    """client-vtable entity with plausible health whose controller handle
    resolves to a controller whose m_hPawn points back at this pawn"""
    if not is_heap_ptr(value):
        return False
    if not (BASE <= live_u64(value) < BASE + IMG_SIZE):
        return False
    max_hp = struct.unpack("<i", struct.pack("<I", live_u32(value + max_health_off)))[0]
    hp = struct.unpack("<i", struct.pack("<I", live_u32(value + health_off)))[0]
    # bonus health from buffs can push current health past the base max field
    if not (100 <= max_hp <= 100000 and 0 < hp <= 100000):
        return False
    controller = resolve_entity(live_u32(value + controller_off), es_value)
    if not controller:
        return False
    back = resolve_entity(live_u32(controller + pawn_handle_off), es_value)
    return back == value

def discover_pawn_global(es_value, health_off, max_health_off,
                         controller_off, pawn_handle_off):
    entities = set()
    for chunk_i in range(8):
        chunk = live_u64(es_value + 0x10 + chunk_i * 8)
        if not is_heap_ptr(chunk):
            break
        for slot in range(512):
            entity = live_u64(chunk + slot * 0x70)
            if entity:
                entities.add(entity)
    matches = []
    for name, start, size in WRITABLE:
        for off in range(start, start + size - 8, 8):
            value = struct.unpack_from("<Q", image, off)[0]
            if value in entities and looks_like_pawn(
                    value, es_value, health_off, max_health_off,
                    controller_off, pawn_handle_off):
                matches.append((off, value))
    return matches

# ---- recv-table field extraction ----
def cstring_at(pos):
    end = image.find(b"\x00", pos)
    if end == -1 or end - pos > 96:
        return None
    try:
        text = image[pos:end].decode("ascii")
    except UnicodeDecodeError:
        return None
    return text if text.isprintable() and text else None

def neighbors_of(site_pos, radius=0x400):
    names = []
    lo = max(0, site_pos - radius)
    hi = min(len(image) - 8, site_pos + radius)
    for off in range(lo, hi, 8):
        v = struct.unpack_from("<Q", image, off)[0]
        if BASE <= v < BASE + IMG_SIZE:
            rel = v - BASE
            if RDATA_LO <= rel < RDATA_HI:
                name = cstring_at(rel)
                if name:
                    names.append(name)
    return names

def entries_for(field_name):
    needle = field_name.encode() + b"\x00"
    entries = []
    spos = image.find(needle)
    while spos != -1:
        if spos == 0 or image[spos - 1] == 0:
            target = BASE + spos
            ppos = image.find(struct.pack("<Q", target))
            while ppos != -1:
                off_val, flags = struct.unpack_from("<II", image, ppos + 0x10)
                if 0x8 <= off_val <= 0x8000:
                    entries.append({"offset": off_val, "flags": flags,
                                    "site": ppos, "near": neighbors_of(ppos)})
                ppos = image.find(struct.pack("<Q", target), ppos + 1)
        spos = image.find(needle, spos + 1)
    return entries

def pick(entries, marker=None, largest=False):
    if marker:
        marked = [e for e in entries if marker in e["near"]]
        if marked:
            return marked[0]
    if largest and entries:
        return max(entries, key=lambda e: e["offset"])
    return entries[0] if entries else None

# ---- extract fields ----
ability_component = pick(entries_for("m_CCitadelAbilityComponent"),
                         marker="m_bLearningAbility")
last_damage = pick(entries_for("m_flLastDamageTime"), marker="CCitadelRecentDamage")
damage_taken = pick(entries_for("m_sPlayerDamageTaken"), marker="m_flLastSpawnTime")

fields = {
    "pawn": {
        "health": pick(entries_for("m_iHealth"), largest=True),
        "max_health": pick(entries_for("m_iMaxHealth"), marker="m_lifeState"),
        "life_state": pick(entries_for("m_lifeState")),
        "team": pick(entries_for("m_iTeamNum"), marker="m_lifeState"),
        "sim_time": pick(entries_for("m_flSimulationTime"), marker="m_CBodyComponent"),
        "controller_handle": pick(entries_for("m_hController"), marker="m_pWeaponServices"),
        "ability_component": ability_component,
        "damage_taken": damage_taken,
    },
    "component": {
        "abilities": pick(entries_for("m_vecAbilities")),
        "interrupt": pick(entries_for("m_bInInterruptState")),
    },
    "ability": {
        "channeling": pick(entries_for("m_bChanneling"), marker="m_flCooldownStart"),
        "cooldown_start": pick(entries_for("m_flCooldownStart")),
        "cooldown_end": pick(entries_for("m_flCooldownEnd"), marker="m_flCooldownStart"),
        "slot": pick(entries_for("m_eAbilitySlot")),
        "charges": pick(entries_for("m_iRemainingCharges"), marker="m_flCooldownStart"),
        "parry_success_end": pick(entries_for("m_flParrySuccessEndTime")),
        "melee_state": pick(entries_for("m_eCurrentAttackState")),
    },
    "controller": {
        "pawn_handle": pick(entries_for("m_hPawn"), marker="Get Player Slot"),
        "player_data": pick(entries_for("m_PlayerDataGlobal"), marker="m_ePlayState"),
    },
    "scoreboard": {
        "hero_id": pick(entries_for("m_nHeroID"), marker="m_iPlayerKills"),
        "health": pick(entries_for("m_iHealth"), marker="m_iPlayerKills"),
        "kills": pick(entries_for("m_iPlayerKills"), marker="m_iHealthMax"),
        "assists": pick(entries_for("m_iPlayerAssists"), marker="m_iHealthMax"),
        "deaths": pick(entries_for("m_iDeaths"), marker="m_iHealthMax"),
        "kill_streak": pick(entries_for("m_iKillStreak"), marker="m_iHealthMax"),
        "alive": pick(entries_for("m_bAlive"), marker="m_iHealthMax"),
    },
}

for group, items in fields.items():
    for name, entry in items.items():
        if entry is None:
            print(f"# MISSING: {group}.{name}", file=sys.stderr)
        else:
            print(f"# {group}.{name} = 0x{entry['offset']:X} (site client+0x{entry['site']:X})",
                  file=sys.stderr)

# ---- globals: discover, fall back to known ----
es_matches = [] if ARGS.static else discover_entity_system()
pawn_fields = fields["pawn"]
health_off = pawn_fields["health"]["offset"] if pawn_fields["health"] else 0x354
max_health_off = pawn_fields["max_health"]["offset"] if pawn_fields["max_health"] else 0x350
controller_off = pawn_fields["controller_handle"]["offset"] if pawn_fields["controller_handle"] else 0x1050
pawn_handle_off = (fields["controller"]["pawn_handle"]["offset"]
                   if fields["controller"]["pawn_handle"] else 0x6BC)

PAWN_GLOBAL = KNOWN_PAWN_GLOBAL
if es_matches:
    es_offset, es_value = es_matches[0]
    if len(es_matches) > 1:
        print(f"# note: {len(es_matches)} entity-system candidates, ranked by vtable diversity",
              file=sys.stderr)
    print(f"# entity system global DISCOVERED at client+0x{es_offset:X} -> 0x{es_value:X}",
          file=sys.stderr)
    ES_GLOBAL = es_offset

    known_value = live_u64(BASE + KNOWN_PAWN_GLOBAL)
    if looks_like_pawn(known_value, es_value, health_off, max_health_off,
                       controller_off, pawn_handle_off):
        print(f"# known pawn global client+0x{KNOWN_PAWN_GLOBAL:X} validated in place",
              file=sys.stderr)
    else:
        pawn_matches = discover_pawn_global(es_value, health_off, max_health_off,
                                            controller_off, pawn_handle_off)
        if pawn_matches:
            PAWN_GLOBAL = pawn_matches[0][0]
            others = ", ".join(f"client+0x{off:X}" for off, _ in pawn_matches[1:5])
            print(f"# pawn global DISCOVERED at client+0x{PAWN_GLOBAL:X}"
                  + (f" (aliases: {others})" if others else ""), file=sys.stderr)
            print(f"# update KNOWN_PAWN_GLOBAL in this script to {PAWN_GLOBAL:#x} for menu fallbacks",
                  file=sys.stderr)
        else:
            print(f"# pawn global not found (menu, or not spawned yet?); keeping known "
                  f"client+0x{KNOWN_PAWN_GLOBAL:X} — get into a match and re-run to confirm",
                  file=sys.stderr)
elif ARGS.static:
    ES_GLOBAL = KNOWN_ES_GLOBAL
    print("# static mode: globals need the live game; keeping KNOWN_* values", file=sys.stderr)
else:
    ES_GLOBAL = KNOWN_ES_GLOBAL
    print(f"# entity system NOT discovered (game still loading?); keeping known"
          f" client+0x{KNOWN_ES_GLOBAL:X} — verify it dereferences to a live object",
          file=sys.stderr)

# ---- emit toml ----
def hexval(entry):
    return f'"{entry["offset"]:#x}"' if entry else None

comp = ability_component["offset"] if ability_component else 0x1440
lines_out = [
    "# deadass-offsets.toml — generated by scripts/schema_extract.py",
    "# from live game memory (recv-table descriptors). Refresh after updates:",
    "#   deadlock running (in a match for pawn discovery), then:",
    "#   python scripts/schema_extract.py > deadass-offsets.toml",
    "",
    "[dll]",
    "port = 24680",
    "poll_interval_ms = 33",
    "",
    "[globals]",
    f'local_pawn = "{PAWN_GLOBAL:#x}"',
    f'entity_system = "{ES_GLOBAL:#x}"',
    "",
    "[entity_list]",
    'chunk_array = "0x10"',
    "chunk_size = 512",
    'stride = "0x70"',
    "",
    "[pawn]",
]
pawn = fields["pawn"]
for key in ("health", "max_health", "life_state", "team", "sim_time", "controller_handle"):
    v = hexval(pawn[key])
    if v:
        lines_out.append(f"{key} = {v}")
if ability_component:
    lines_out.append(f'ability_component = "{comp:#x}"')
if ability_component and fields["component"]["abilities"]:
    lines_out.append(f'abilities = "{comp + fields["component"]["abilities"]["offset"]:#x}"')
if ability_component and fields["component"]["interrupt"]:
    lines_out.append(f'interrupt_state = "{comp + fields["component"]["interrupt"]["offset"]:#x}"')
if damage_taken and last_damage:
    lines_out.append(f'damage_taken_time = "{damage_taken["offset"] + last_damage["offset"]:#x}"')

lines_out += ["", "[ability]"]
for key in ("channeling", "cooldown_start", "cooldown_end", "slot",
            "charges", "parry_success_end", "melee_state"):
    v = hexval(fields["ability"][key])
    if v:
        lines_out.append(f"{key} = {v}")
# gameplay constants, not schema-derived: the weapon-melee slot and the
# highest slot tracked for parry detection
lines_out.append("melee_slot = 22")
lines_out.append("max_slot = 22")

lines_out += ["", "[controller]"]
controller = fields["controller"]
if controller["player_data"]:
    lines_out.append(f'player_data = "{controller["player_data"]["offset"]:#x}"')
if controller["pawn_handle"]:
    lines_out.append(f'pawn_handle = "{controller["pawn_handle"]["offset"]:#x}"')
score = fields["scoreboard"]
for key in ("health", "hero_id", "kills", "assists", "deaths", "kill_streak", "alive"):
    v = hexval(score[key])
    if v:
        lines_out.append(f"{key} = {v}")

print("\n".join(lines_out))
