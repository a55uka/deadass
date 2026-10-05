"""Probe the live game to find current entity system and scoreboard offsets.

Strategy:
1. Scan .data section for the entity system: look for a pointer whose target
   has the entity list chunk array at +0x10 (verified layout).
2. Walk the entity list to find player controllers.
3. Read their scoreboard counters to verify.
"""

import ctypes, ctypes.wintypes as wt, struct, subprocess

k32 = ctypes.WinDLL("kernel32", use_last_error=True)
k32.OpenProcess.restype = ctypes.c_void_p
k32.OpenProcess.argtypes = [wt.DWORD, wt.BOOL, wt.DWORD]
k32.ReadProcessMemory.restype = wt.BOOL
k32.ReadProcessMemory.argtypes = [ctypes.c_void_p, ctypes.c_void_p, ctypes.c_void_p, ctypes.c_size_t, ctypes.POINTER(ctypes.c_size_t)]

psapi = ctypes.WinDLL("psapi")
psapi.EnumProcessModulesEx.restype = wt.BOOL
psapi.EnumProcessModulesEx.argtypes = [ctypes.c_void_p, ctypes.POINTER(ctypes.c_void_p), wt.DWORD, ctypes.POINTER(wt.DWORD), wt.DWORD]
psapi.GetModuleFileNameExW.restype = wt.DWORD
psapi.GetModuleFileNameExW.argtypes = [ctypes.c_void_p, ctypes.c_void_p, ctypes.c_wchar_p, wt.DWORD]

out = subprocess.run(["tasklist", "/FI", "IMAGENAME eq deadlock.exe", "/FO", "CSV"], capture_output=True, text=True).stdout
pid = int(out.splitlines()[1].split('","')[1])
process = k32.OpenProcess(0x0410, False, pid)

needed = wt.DWORD(0)
psapi.EnumProcessModulesEx(process, None, 0, ctypes.byref(needed), 0x03)
mods = (ctypes.c_void_p * (needed.value // 8))()
psapi.EnumProcessModulesEx(process, mods, needed.value, ctypes.byref(needed), 0x03)
BASE = None
for mod in mods:
    name = ctypes.create_unicode_buffer(512)
    psapi.GetModuleFileNameExW(process, mod, name, 512)
    if name.value.lower().endswith("\\client.dll"):
        BASE = ctypes.cast(mod, ctypes.c_void_p).value
print(f"pid={pid} client.dll base=0x{BASE:X}")

def read(address, size):
    buf = ctypes.create_string_buffer(size)
    got = ctypes.c_size_t(0)
    if not k32.ReadProcessMemory(process, ctypes.c_void_p(address), buf, size, ctypes.byref(got)):
        return None
    return buf.raw[:got.value]

def u64(addr):
    raw = read(addr, 8)
    return struct.unpack("<Q", raw)[0] if raw else 0

def u32(addr):
    raw = read(addr, 4)
    return struct.unpack("<I", raw)[0] if raw else 0

def i32(addr):
    raw = read(addr, 4)
    return struct.unpack("<i", raw)[0] if raw else None

def u8(addr):
    raw = read(addr, 1)
    return raw[0] if raw else 0

# Read PE to get .data section bounds
e_lfanew = u32(BASE + 0x3C)
num_sections = u32(BASE + e_lfanew + 6)
opt_size = u32(BASE + e_lfanew + 0x14)
sec_table = BASE + e_lfanew + 0x18 + opt_size
data_start = data_end = 0
for si in range(num_sections):
    s = sec_table + si * 40
    sec_name = read(s, 8)
    if sec_name:
        name = sec_name.rstrip(b"\x00").decode()
        vsize, va = struct.unpack_from("<II", read(s + 8, 8), 0)
        if name == ".data":
            data_start = BASE + va
            data_end = data_start + vsize
            print(f".data: 0x{data_start:X} - 0x{data_end:X} ({vsize/1e6:.1f} MB)")
            break

# Read local pawn global (verified offset from Sep 20 dump)
PAWN_GLOBAL = 0x2F194F8
pawn = u64(BASE + PAWN_GLOBAL)
print(f"local_pawn (0x{PAWN_GLOBAL:X}): 0x{pawn:X}")
print(f"  plausible heap: {0x10000 < pawn < 0x7FFFFFFEFFFF}")
if pawn:
    print(f"  vtable: 0x{u64(pawn):X}")

# Read the CEngineClient global
EC_GLOBAL = 0x37FCF38
ec = u64(BASE + EC_GLOBAL)
print(f"CEngineClient (0x{EC_GLOBAL:X}): 0x{ec:X}")

# Now: scan .data for the entity system. We know it's a pointer to an object
# whose +0x10 field is a chunk array pointer, and chunk[0] + 0x70*0 has a
# valid entity. We search for this pattern.
print("\nscanning .data for entity system pointer...")

# Read the entire .data section into memory (it's ~12MB)
data_blob = read(data_start, data_size)
if data_blob:
    # Look for pointers where: *ptr != 0, *(ptr + 0x10) != 0, and walking
    # the chunk at *(ptr + 0x10) gives us entities
    found = []
    for off in range(0, len(data_blob) - 8, 8):
        val = struct.unpack_from("<Q", data_blob, off)[0]
        if not (0x10000 < val < 0x7FFFFFFEFFFF):
            continue
        # Try reading the object at val
        chunk_array = u64(val + 0x10)
        if not chunk_array or chunk_array < 0x10000:
            continue
        # Try walking chunk[0]
        c0 = u64(chunk_array)
        if not c0 or c0 < 0x10000:
            continue
        # Read first entity from chunk[0] + 0
        e0 = u64(c0)
        if not e0 or e0 < 0x10000:
            continue
        # Check if e0 has a vtable in a module
        vt = u64(e0)
        if not (0x10000 < vt < 0x7FFFFFFEFFFF):
            continue
        # This looks like a valid entity system!
        rva = data_start - BASE + off
        found.append((rva, val))
        if len(found) >= 3:
            break

    for rva, val in found:
        print(f"  candidate entity system: .data rva 0x{rva:X} -> 0x{val:X}")
        # Walk it and count entities
        count = 0
        for ci in range(4):
            chunk = u64(val + 0x10 + 8 * ci)
            if not chunk: break
            blob = read(chunk, 512 * 0x70)
            if not blob: continue
            for slot in range(512):
                inst = struct.unpack_from("<Q", blob, slot * 0x70)[0]
                if inst:
                    count += 1
        print(f"    entities: {count}")
else:
    print("could not read .data")

# Also scan for the local pawn (player with nonzero health)
# Known: CEngineClient global at 0x37FCF38 was verified
ec = u64(BASE + 0x37FCF38)
print(f"\nCEngineClient (0x{EC_GLOBAL:X}): 0x{ec:X}")
PYEOF