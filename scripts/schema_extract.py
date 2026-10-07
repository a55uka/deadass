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
    entity list whose controller handle resolves back to a controller with
    the same vtable. Needs a live match; in the menu the last known value
    is kept.

Usage (game running):
    python scripts/schema_extract.py > deadass-offsets.toml
Field candidates and discovery notes go to stderr.
"""

import ctypes, ctypes.wintypes as wt, struct, subprocess, sys

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

# Fallbacks used only when auto-discovery cannot run (e.g. pawn discovery
# needs a live match). Update after a fresh dump if you skip discovery.
KNOWN_PAWN_GLOBAL = 0x32C59D8
KNOWN_ES_GLOBAL = 0x3474830

out = subprocess.run(["tasklist", "/FI", "IMAGENAME eq deadlock.exe", "/FO", "CSV"], capture_output=True, text=True).stdout
lines = [l for l in out.splitlines() if "deadlock" in l.lower()]
if not lines:
    sys.exit("deadlock.exe is not running")
pid = int(lines[0].split('","')[1])
process = k32.OpenProcess(0x0410, False, pid)
needed = wt.DWORD(0)
psapi.EnumProcessModulesEx(process, None, 0, ctypes.byref(needed), 0x03)
mods = (ctypes.c_void_p * (needed.value // 8))()
psapi.EnumProcessModulesEx(process, mods, needed.value, ctypes.byref(needed), 0x03)
MODULE_BASES = []
BASE = None
for mod in mods:
    name = ctypes.create_unicode_buffer(512)
    psapi.GetModuleFileNameExW(process, mod, name, 512)
    MODULE_BASES.append(ctypes.cast(mod, ctypes.c_void_p).value)
    if name.value.lower().endswith("\\client.dll"):
        BASE = ctypes.cast(mod, ctypes.c_void_p).value
if BASE is None:
    sys.exit("client.dll not loaded yet")

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

def live_u64(addr):
    raw = read(addr, 8)
    return struct.unpack("<Q", raw)[0] if raw else 0

def live_u32(addr):
    raw = read(addr, 4)
    return struct.unpack("<I", raw)[0] if raw else 0

def in_module(addr):
    # generous 128MB span per module; exact PE sizes are not needed here
    return any(base <= addr < base + 0x8000000 for base in MODULE_BASES)

def is_heap_ptr(value):
    return 0x10000000000 <= value < 0x7FF000000000 and committed(value) and not in_module(value)

e_lfanew = struct.unpack("<I", read(BASE + 0x3C, 4))[0]
IMG_SIZE = struct.unpack("<I", read(BASE + e_lfanew + 0x50, 4))[0]
image = read(BASE, IMG_SIZE)
if not image:
    sys.exit("cannot read client.dll image")
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
            if 0x10000000000 <= value < 0x7FF000000000 and entity_system_looks_like(value):
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

def looks_like_pawn(value, es_value, health_off, max_health_off, controller_off):
    """client-vtable entity with plausible health and a controller handle
    that resolves to an entity of the same class"""
    if not is_heap_ptr(value):
        return False
    if not (BASE <= live_u64(value) < BASE + IMG_SIZE):
        return False
    max_hp = struct.unpack("<i", struct.pack("<I", live_u32(value + max_health_off)))[0]
    hp = struct.unpack("<i", struct.pack("<I", live_u32(value + health_off)))[0]
    if not (100 <= max_hp <= 100000 and 0 < hp <= max_hp):
        return False
    controller = resolve_entity(live_u32(value + controller_off), es_value)
    return bool(controller) and vtable_of(controller) == live_u64(value)

def discover_pawn_global(es_value, health_off, max_health_off, controller_off):
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
                    value, es_value, health_off, max_health_off, controller_off):
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
            if 0x2500000 < rel < 0x2C00000:
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
es_matches = discover_entity_system()
pawn_fields = fields["pawn"]
health_off = pawn_fields["health"]["offset"] if pawn_fields["health"] else 0x354
max_health_off = pawn_fields["max_health"]["offset"] if pawn_fields["max_health"] else 0x350
controller_off = pawn_fields["controller_handle"]["offset"] if pawn_fields["controller_handle"] else 0x1050

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
    if looks_like_pawn(known_value, es_value, health_off, max_health_off, controller_off):
        print(f"# known pawn global client+0x{KNOWN_PAWN_GLOBAL:X} validated in place",
              file=sys.stderr)
    else:
        pawn_matches = discover_pawn_global(es_value, health_off, max_health_off, controller_off)
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
