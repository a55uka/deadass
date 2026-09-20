#[cfg(windows)]
pub use windows_impl::{client_base, pages_committed, snapshot};

use super::diff::{PawnSnapshot, PlayerSnapshot, Snapshot};
use super::offsets::Offsets;

/// Highest entity index we will ever chase when resolving ability handles.
const MAX_ENTITY_INDEX: u64 = 1 << 15;
/// Sanity cap for the abilities vector length.
const MAX_ABILITIES: u32 = 32;

const MIN_USER_ADDRESS: u64 = 0x10000;
const MAX_USER_ADDRESS: u64 = 0x7FFF_FFFE_FFFF;

/// Pure guard for [`read_bytes`], so the bounds stay unit tested.
fn is_readable(address: u64, len: usize) -> bool {
    let end = match address.checked_add(len as u64) {
        Some(end) => end,
        None => return false,
    };
    address >= MIN_USER_ADDRESS && end <= MAX_USER_ADDRESS
}

#[cfg(windows)]
mod windows_impl {
    use super::*;
    use std::sync::OnceLock;
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::System::Memory::{VirtualQueryEx, MEMORY_BASIC_INFORMATION, MEM_COMMIT};

    /// Base address of client.dll inside this process, or null before the
    /// client module exists.
    pub fn client_base() -> u64 {
        let name: Vec<u16> = "client.dll\0".encode_utf16().collect();
        let handle = unsafe { GetModuleHandleW(name.as_ptr()) };
        handle as u64
    }

    fn image_bounds() -> (u64, u64) {
        static BOUNDS: OnceLock<(u64, u64)> = OnceLock::new();
        if let Some(bounds) = BOUNDS.get() {
            return *bounds;
        }
        let base = client_base();
        if base == 0 {
            return (0, 0);
        }
        // PE header: e_lfanew at +0x3C, SizeOfImage in the optional header.
        let e_lfanew = u32_at(base + 0x3C).unwrap_or(0) as u64;
        let size = u32_at(base + e_lfanew + 0x50).unwrap_or(0) as u64;
        if size == 0 || size > (1 << 30) {
            return (0, 0);
        }
        let bounds = (base, base.saturating_add(size));
        let _ = BOUNDS.set(bounds);
        bounds
    }

    fn u32_at(address: u64) -> Option<u32> {
        let slice = unsafe { std::slice::from_raw_parts(address as *const u8, 4) };
        Some(u32::from_le_bytes(slice.try_into().ok()?))
    }

    fn in_client_image(addr: u64) -> bool {
        let (base, end) = image_bounds();
        base != 0 && addr >= base && addr < end
    }

    /// An entity pointer is trusted only when its first qword (vtable) is a
    /// client.dll address — the engine keeps vtables in .rdata, and destroyed
    /// or half-constructed entities fail this check.
    fn entity_pointer_valid(entity: u64) -> bool {
        entity_pointer_vtable(entity).is_some()
    }

    fn entity_pointer_vtable(entity: u64) -> Option<u64> {
        let raw = read_bytes(entity, 8)?;
        let vtable = u64::from_le_bytes(raw.try_into().ok()?);
        in_client_image(vtable).then_some(vtable)
    }

    /// Every byte we dereference must sit in committed, accessible memory.
    /// The game frees entity chunks and pawns during map transitions; a
    /// dangling pointer into released memory would otherwise fault and take
    /// the whole process down, so pages are verified before every read.
    pub fn pages_committed(address: u64, len: usize) -> bool {
        use windows_sys::Win32::System::Memory::PAGE_GUARD;
        use windows_sys::Win32::System::Memory::PAGE_NOACCESS;
        use windows_sys::Win32::System::Threading::GetCurrentProcess;

        const PAGE_GUARD_FLAG: u32 = PAGE_GUARD;
        const PAGE_NOACCESS_FLAG: u32 = PAGE_NOACCESS;

        let Some(end) = address.checked_add(len as u64) else {
            return false;
        };
        let mut cursor = address;
        let mut info: MEMORY_BASIC_INFORMATION = unsafe { std::mem::zeroed() };
        let info_size = std::mem::size_of::<MEMORY_BASIC_INFORMATION>();
        while cursor < end {
            let process = unsafe { GetCurrentProcess() };
            let written =
                unsafe { VirtualQueryEx(process, cursor as *const _, &mut info, info_size) };
            if written == 0 {
                return false;
            }
            let region_end = cursor.saturating_add(info.RegionSize as u64);
            if region_end <= cursor {
                return false;
            }
            if info.State != MEM_COMMIT
                || (info.Protect & PAGE_GUARD_FLAG) != 0
                || info.Protect == PAGE_NOACCESS_FLAG
            {
                return false;
            }
            cursor = region_end;
        }
        true
    }

    fn read_bytes(address: u64, len: usize) -> Option<&'static [u8]> {
        if !is_readable(address, len) || !pages_committed(address, len) {
            return None;
        }
        let slice = unsafe { std::slice::from_raw_parts(address as *const u8, len) };
        Some(slice)
    }

    #[cfg(test)]
    mod page_tests {
        use super::*;
        use windows_sys::Win32::System::Memory::{
            VirtualAlloc, VirtualFree, MEM_COMMIT, MEM_DECOMMIT, MEM_RELEASE, MEM_RESERVE,
            PAGE_READWRITE,
        };

        #[test]
        fn freed_pages_are_not_readable() {
            unsafe {
                let page = VirtualAlloc(
                    std::ptr::null(),
                    0x10000,
                    MEM_RESERVE | MEM_COMMIT,
                    PAGE_READWRITE,
                );
                assert!(!page.is_null(), "allocation failed");
                let address = page as u64;
                assert!(pages_committed(address, 16));
                assert!(read_bytes(address, 8).is_some());

                // Decommitted memory must be rejected instead of faulting —
                // this is the crash class seen on lobby->sandbox transitions.
                assert_ne!(VirtualFree(page as _, 0x10000, MEM_DECOMMIT), 0);
                assert!(!pages_committed(address, 16));
                assert!(read_bytes(address, 8).is_none());

                assert_ne!(VirtualFree(page as _, 0, MEM_RELEASE), 0);
                assert!(!pages_committed(address, 16));
            }
        }
    }

    fn read_u8(address: u64) -> Option<u8> {
        Some(read_bytes(address, 1)?[0])
    }

    fn read_u16(address: u64) -> Option<u16> {
        Some(u16::from_le_bytes(read_bytes(address, 2)?.try_into().ok()?))
    }

    fn read_u32(address: u64) -> Option<u32> {
        Some(u32::from_le_bytes(read_bytes(address, 4)?.try_into().ok()?))
    }

    fn read_i32(address: u64) -> Option<i32> {
        Some(i32::from_le_bytes(read_bytes(address, 4)?.try_into().ok()?))
    }

    fn read_f32(address: u64) -> Option<f32> {
        Some(f32::from_le_bytes(read_bytes(address, 4)?.try_into().ok()?))
    }

    fn read_u64(address: u64) -> Option<u64> {
        Some(u64::from_le_bytes(read_bytes(address, 8)?.try_into().ok()?))
    }

    /// Resolve a Source 2 entity handle (index in the low 15 bits) through the
    /// entity system's chunked list: chunk array at `entity_system +
    /// entity_chunk_array`, 8-byte chunk pointers, `entity_stride`-byte slots.
    fn resolve_entity(handle: u32, offsets: &Offsets, entity_system: u64) -> Option<u64> {
        let index = (handle & 0x7FFF) as u64;
        if index >= MAX_ENTITY_INDEX || entity_system == 0 {
            return None;
        }
        let chunk_array = entity_system.checked_add(offsets.entity_chunk_array)?;
        let chunk = read_u64(chunk_array.checked_add((index / offsets.entity_chunk_size).checked_mul(8)?)?)?;
        if chunk == 0 {
            return None;
        }
        let slot = index % offsets.entity_chunk_size;
        read_u64(chunk.checked_add(slot.checked_mul(offsets.entity_stride)?)?)
    }

    pub fn snapshot(offsets: &Offsets) -> Snapshot {
        // One-shot sanity report on the configured entity-system global: a
        // stale value (game patch) reads as an invalid entity system, and
        // this logs that immediately instead of failing silently.
        static GLOBAL_CHECKED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
        if GLOBAL_CHECKED.get().is_none() {
            let valid = entity_system_ptr(offsets).is_some();
            let _ = GLOBAL_CHECKED.set(());
            crate::debug_log::always(&format!(
                "configured entity_system global {:#x} {}",
                offsets.entity_system_global,
                if valid { "valid" } else { "INVALID (stale? update deadass-offsets.toml)" }
            ));
        }

        let pawn = self_pawn(offsets);
        let players = pawn
            .as_ref()
            .and_then(|pawn| read_players(pawn.address, offsets))
            .unwrap_or_default();
        Snapshot { pawn, players }
    }

    fn self_pawn(offsets: &Offsets) -> Option<PawnSnapshot> {
        let client = client_base();
        if client == 0 {
            return None;
        }
        let pawn = match read_u64(client.checked_add(offsets.local_pawn_global)?) {
            Some(pawn) if pawn != 0 && entity_pointer_valid(pawn) => pawn,
            _ => return None,
        };
        let health = read_i32(pawn.checked_add(offsets.pawn_health)?)?;
        let life_state = read_u8(pawn.checked_add(offsets.pawn_life_state)?)?;
        let game_time = read_f32(pawn.checked_add(offsets.pawn_sim_time)?).unwrap_or(0.0);
        let interrupted = read_u8(pawn.checked_add(offsets.pawn_interrupt_state)?)
            .is_some_and(|raw| raw != 0);
        let damage_taken_time =
            read_f32(pawn.checked_add(offsets.pawn_damage_taken_time)?).unwrap_or(0.0);

        let abilities = read_abilities(pawn, offsets);
        Some(PawnSnapshot {
            address: pawn,
            health,
            life_state,
            game_time,
            interrupted,
            damage_taken_time,
            abilities,
        })
    }

    /// The configured entity-system global, structurally validated: the
    /// referenced instance must be a client.dll object whose chunk pointer
    /// array (inline at instance + chunk_array) dereferences to real
    /// entities. A stale global after a game patch cannot fake that.
    fn entity_system_ptr(offsets: &Offsets) -> Option<u64> {
        let client = client_base();
        if client == 0 {
            return None;
        }
        let entity_system = read_u64(client.checked_add(offsets.entity_system_global)?)?;
        if entity_system == 0 || !entity_pointer_valid(entity_system) {
            return None;
        }
        let Some(chunk_array) =
            read_u64(entity_system.checked_add(offsets.entity_chunk_array)?)
        else {
            return None;
        };
        if chunk_array == 0 || in_client_image(chunk_array) {
            return None;
        }
        (0..16u64)
            .any(|slot| {
                read_u64(chunk_array + slot * offsets.entity_stride)
                    .is_some_and(|entity| entity != 0 && entity_pointer_valid(entity))
            })
            .then_some(entity_system)
    }

    fn read_abilities(pawn: u64, offsets: &Offsets) -> Vec<super::super::diff::AbilitySnapshot> {
        let mut found = Vec::new();
        let Some(entity_system) = entity_system_ptr(offsets) else {
            return found;
        };
        let vector = pawn.checked_add(offsets.abilities_vector).unwrap_or(0);
        let (Some(count), Some(data)) = (
            read_u32(vector),
            read_u64(vector.checked_add(8).unwrap_or(0)),
        ) else {
            return found;
        };
        if count == 0 || count > MAX_ABILITIES || data == 0 {
            return found;
        }
        for slot_index in 0..count {
            let element = data + (slot_index as u64) * 4;
            let Some(handle) = read_u32(element) else {
                continue;
            };
            let Some(entity) = resolve_entity(handle, offsets, entity_system) else {
                continue;
            };
            if entity == 0 || !entity_pointer_valid(entity) {
                continue;
            }
            let (Some(slot), Some(cooldown_end), Some(charges)) = (
                read_u16(entity.checked_add(offsets.ability_slot).unwrap_or(0)),
                read_f32(entity.checked_add(offsets.ability_cooldown_end).unwrap_or(0)),
                read_i32(entity.checked_add(offsets.ability_charges).unwrap_or(0)),
            ) else {
                continue;
            };
            let attack_parried = read_u8(entity.checked_add(offsets.ability_attack_parried).unwrap_or(0))
                .is_some_and(|raw| raw != 0);
            let parry_start =
                read_f32(entity.checked_add(offsets.ability_parry_start).unwrap_or(0))
                    .unwrap_or(0.0);
            let parry_success_end =
                read_f32(entity.checked_add(offsets.ability_parry_success_end).unwrap_or(0))
                    .unwrap_or(0.0);
            let melee_state =
                read_u32(entity.checked_add(offsets.ability_melee_state).unwrap_or(0)).unwrap_or(0);
            let melee_chain =
                read_u32(entity.checked_add(offsets.ability_melee_chain).unwrap_or(0)).unwrap_or(0);
            if slot as u8 > offsets.max_ability_slot {
                continue;
            }
            // Charges of -1 (or worse) mark abilities without a charge system.
            let charges = if charges >= 0 { Some(charges) } else { None };
            let channeling = read_u8(entity.checked_add(offsets.ability_channeling).unwrap_or(0))
                .is_some_and(|raw| raw != 0);
            found.push(super::super::diff::AbilitySnapshot {
                slot: slot as u8,
                charges,
                cooldown_end,
                channeling,
                attack_parried,
                parry_start,
                parry_success_end,
                melee_state,
                melee_chain,
            });
        }
        found
    }
    
    fn player_melee_chain(controller: u64, offsets: &Offsets, entity_system: u64) -> u32 {
        let Some(handle) =
            read_u32(controller.checked_add(offsets.controller_pawn_handle).unwrap_or(0))
        else {
            return 0;
        };
        let Some(pawn) = resolve_entity(handle, offsets, entity_system) else {
            return 0;
        };
        if pawn == 0 || !entity_pointer_valid(pawn) {
            return 0;
        }
        let vector = pawn.checked_add(offsets.abilities_vector).unwrap_or(0);
        let (Some(count), Some(data)) = (
            read_u32(vector),
            read_u64(vector.checked_add(8).unwrap_or(0)),
        ) else {
            return 0;
        };
        if count == 0 || count > MAX_ABILITIES || data == 0 {
            return 0;
        }
        for slot_index in 0..count {
            let offset = (slot_index as u64).checked_mul(4).unwrap_or(u64::MAX);
            let Some(handle) = read_u32(data.checked_add(offset).unwrap_or(u64::MAX)) else {
                continue;
            };
            let Some(entity) = resolve_entity(handle, offsets, entity_system) else {
                continue;
            };
            if entity == 0 {
                continue;
            }
            let Some(slot) = read_u16(entity.checked_add(offsets.ability_slot).unwrap_or(0)) else {
                continue;
            };
            if slot as u8 != offsets.melee_slot {
                continue;
            }
            return read_u32(entity.checked_add(offsets.ability_melee_chain).unwrap_or(0))
                .unwrap_or(0);
        }
        0
    }
    
    fn read_players(pawn: u64, offsets: &Offsets) -> Option<Vec<PlayerSnapshot>> {
        let entity_system = entity_system_ptr(offsets)?;
        let handle = read_u32(pawn.checked_add(offsets.pawn_controller_handle)?)?;
        let local_controller = resolve_entity(handle, offsets, entity_system)?;
        if local_controller == 0 || !entity_pointer_valid(local_controller) {
            return None;
        }
        let controller_vtable = read_u64(local_controller)?;
        if !in_client_image(controller_vtable) {
            return None;
        }

        let mut players = Vec::new();
        for index in 0..MAX_ENTITY_INDEX.min(offsets.entity_chunk_size * 8) {
            if players.len() >= 16 {
                break;
            }
            let chunk = read_u64(
                entity_system
                    .checked_add(offsets.entity_chunk_array)?
                    .checked_add((index / offsets.entity_chunk_size).checked_mul(8)?)?,
            )?;
            if chunk == 0 {
                break;
            }
            let entity = read_u64(
                chunk.checked_add((index % offsets.entity_chunk_size).checked_mul(offsets.entity_stride)?)?,
            )?;
            if entity == 0 || !entity_pointer_valid(entity) {
                continue;
            }
            if entity_pointer_vtable(entity) != Some(controller_vtable) {
                continue;
            }
            let data = |field: u64| entity.checked_add(offsets.controller_player_data)?.checked_add(field);
            let (Some(hero_id), Some(kills), Some(assists), Some(deaths), Some(streak), Some(alive), Some(health)) = (
                read_u32(data(offsets.player_hero_id)?),
                read_i32(data(offsets.player_kills)?),
                read_i32(data(offsets.player_assists)?),
                read_i32(data(offsets.player_deaths)?),
                read_i32(data(offsets.player_kill_streak)?),
                read_u8(data(offsets.player_alive)?),
                read_i32(data(offsets.player_health)?),
            ) else {
                continue;
            };
            let melee_chain = player_melee_chain(entity, offsets, entity_system);
            players.push(PlayerSnapshot {
                address: entity,
                is_local: entity == local_controller,
                hero_id,
                kills,
                assists,
                deaths,
                kill_streak: streak,
                alive: alive != 0,
                health,
                melee_chain,
            });
        }
        Some(players)
    }
}

#[cfg(not(windows))]
mod stub_impl {
    use super::*;

    pub fn snapshot(_offsets: &Offsets) -> Snapshot {
        Snapshot::default()
    }

    pub fn client_base() -> u64 {
        0
    }
}

#[cfg(not(windows))]
pub use stub_impl::{client_base, snapshot};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_x64_module_addresses_are_readable() {
        // Typical ASLR'd module base, well past 32-bit space.
        assert!(is_readable(0x7FFA_5C7E_0000, 8));
        // Typical 32-bit-era base and heap addresses.
        assert!(is_readable(0x140_0000, 4));
    }

    #[test]
    fn garbage_pointers_are_rejected() {
        assert!(!is_readable(0, 8));
        assert!(!is_readable(0xFFF, 8));
        // Kernel space.
        assert!(!is_readable(0xFFFF_8000_0000_0000, 8));
        // Wrap-around.W
        assert!(!is_readable(u64::MAX, 16));
    }
}
