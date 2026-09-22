use std::sync::OnceLock;

use super::entities::{
    client_base, entity_pointer_valid, entity_pointer_vtable, in_client_image, resolve_entity,
};
use super::memory::{read_f32, read_i32, read_u8, read_u16, read_u32, read_u64};
use crate::debug_log;
use crate::diff::{AbilitySnapshot, PawnSnapshot, PlayerSnapshot, Snapshot};
use crate::offsets::Offsets;

/// Sanity cap for the abilities vector length
const MAX_ABILITIES: u32 = 32;

pub fn snapshot(offsets: &Offsets) -> Snapshot {
    static GLOBAL_CHECKED: OnceLock<()> = OnceLock::new();
    if GLOBAL_CHECKED.get().is_none() {
        let valid = entity_system_ptr(offsets).is_some();
        let _ = GLOBAL_CHECKED.set(());
        debug_log::always(&format!(
            "configured entity_system global {:#x} {}",
            offsets.entity_system_global,
            if valid {
                "valid"
            } else {
                "INVALID (stale? update deadass-offsets.toml)"
            }
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
    let interrupted =
        read_u8(pawn.checked_add(offsets.pawn_interrupt_state)?).is_some_and(|raw| raw != 0);
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

/// The configured entity-system global: the
/// referenced instance must be a client.dll object whose chunk pointer
/// array (inline at instance + chunk_array) dereferences to real
/// entities. A stale global after a game patch cannot fake that
fn entity_system_ptr(offsets: &Offsets) -> Option<u64> {
    let client = client_base();
    if client == 0 {
        return None;
    }
    let entity_system = read_u64(client.checked_add(offsets.entity_system_global)?)?;
    if entity_system == 0 || !entity_pointer_valid(entity_system) {
        return None;
    }
    let chunk_array = read_u64(entity_system.checked_add(offsets.entity_chunk_array)?)?;
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

fn read_abilities(pawn: u64, offsets: &Offsets) -> Vec<AbilitySnapshot> {
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
            read_f32(
                entity
                    .checked_add(offsets.ability_cooldown_end)
                    .unwrap_or(0),
            ),
            read_i32(entity.checked_add(offsets.ability_charges).unwrap_or(0)),
        ) else {
            continue;
        };
        let attack_parried = read_u8(
            entity
                .checked_add(offsets.ability_attack_parried)
                .unwrap_or(0),
        )
        .is_some_and(|raw| raw != 0);
        let parry_start =
            read_f32(entity.checked_add(offsets.ability_parry_start).unwrap_or(0)).unwrap_or(0.0);
        let parry_success_end = read_f32(
            entity
                .checked_add(offsets.ability_parry_success_end)
                .unwrap_or(0),
        )
        .unwrap_or(0.0);
        let melee_state =
            read_u32(entity.checked_add(offsets.ability_melee_state).unwrap_or(0)).unwrap_or(0);
        let melee_chain =
            read_u32(entity.checked_add(offsets.ability_melee_chain).unwrap_or(0)).unwrap_or(0);
        if slot as u8 > offsets.max_ability_slot {
            continue;
        }
        let charges = if charges >= 0 { Some(charges) } else { None };
        let channeling = read_u8(entity.checked_add(offsets.ability_channeling).unwrap_or(0))
            .is_some_and(|raw| raw != 0);
        found.push(AbilitySnapshot {
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
    let Some(handle) = read_u32(
        controller
            .checked_add(offsets.controller_pawn_handle)
            .unwrap_or(0),
    ) else {
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
        let offset = (slot_index as u64).saturating_mul(4);
        let Some(handle) = read_u32(data.saturating_add(offset)) else {
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
        return read_u32(entity.checked_add(offsets.ability_melee_chain).unwrap_or(0)).unwrap_or(0);
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
    for index in 0..super::entities::MAX_ENTITY_INDEX.min(offsets.entity_chunk_size * 8) {
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
        let entity = read_u64(chunk.checked_add(
            (index % offsets.entity_chunk_size).checked_mul(offsets.entity_stride)?,
        )?)?;
        if entity == 0 || !entity_pointer_valid(entity) {
            continue;
        }
        if entity_pointer_vtable(entity) != Some(controller_vtable) {
            continue;
        }
        let data = |field: u64| {
            entity
                .checked_add(offsets.controller_player_data)?
                .checked_add(field)
        };
        let (
            Some(hero_id),
            Some(kills),
            Some(assists),
            Some(deaths),
            Some(streak),
            Some(alive),
            Some(health),
        ) = (
            read_u32(data(offsets.player_hero_id)?),
            read_i32(data(offsets.player_kills)?),
            read_i32(data(offsets.player_assists)?),
            read_i32(data(offsets.player_deaths)?),
            read_i32(data(offsets.player_kill_streak)?),
            read_u8(data(offsets.player_alive)?),
            read_i32(data(offsets.player_health)?),
        )
        else {
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
