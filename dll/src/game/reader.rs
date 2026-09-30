use std::collections::BTreeMap;

use super::entities::{
    client_base, entity_pointer_valid, entity_pointer_vtable, in_client_image, resolve_entity,
    MAX_ENTITY_INDEX,
};
use super::memory::{read_f32, read_i32, read_u8, read_u16, read_u32, read_u64};
use crate::debug_log;
use crate::diff::{AbilitySnapshot, PawnSnapshot, PlayerSnapshot, Snapshot};
use crate::offsets::Offsets;

/// Sanity cap for the abilities vector length
const MAX_ABILITIES: u32 = 32;
const MELEE_SWING_FRESH_SECONDS: f32 = 1.0;
const DAMAGE_EDGE_EPSILON: f32 = 0.05;

pub struct Reader {
    last_damage_time: f32,
    damage_advanced: bool,
    melee_entities: BTreeMap<u64, u32>,
    entity_system_was_available: bool,
}

impl Default for Reader {
    fn default() -> Self {
        Self::new()
    }
}

impl Reader {
    pub fn new() -> Self {
        Self {
            last_damage_time: 0.0,
            damage_advanced: false,
            melee_entities: BTreeMap::new(),
            entity_system_was_available: false,
        }
    }

    pub fn snapshot(&mut self, offsets: &Offsets) -> Snapshot {
        let entity_system = Self::entity_system_ptr(offsets);
        let available = entity_system.is_some();
        if available != self.entity_system_was_available {
            debug_log::always(&format!(
                "entity_system global {:#x} {}",
                offsets.entity_system_global,
                if available {
                    "valid"
                } else {
                    "unavailable (in menu, or stale offset)"
                }
            ));
            self.entity_system_was_available = available;
        }

        let mut pawn = self.self_pawn(offsets);
        let players = match (&pawn, entity_system) {
            (Some(pawn), Some(entity_system)) => {
                self.read_players(pawn.address, entity_system, offsets)
            }
            _ => None,
        }
        .unwrap_or_default();

        let melee_threat = self.damage_advanced
            && players
                .iter()
                .filter(|player| !player.is_local)
                .any(|player| {
                    entity_system.is_some_and(|entity_system| {
                        self.player_melee_swinging(
                            player.address,
                            entity_system,
                            offsets,
                            pawn.as_ref().map_or(0.0, |pawn| pawn.game_time),
                        )
                    })
                });
        self.damage_advanced = false;
        if let Some(pawn) = &mut pawn {
            pawn.melee_threat = melee_threat;
        }

        Snapshot { pawn, players }
    }

    fn self_pawn(&mut self, offsets: &Offsets) -> Option<PawnSnapshot> {
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
        self.damage_advanced = damage_taken_time > self.last_damage_time + DAMAGE_EDGE_EPSILON;
        self.last_damage_time = damage_taken_time;

        let abilities = Self::read_abilities(pawn, offsets);
        Some(PawnSnapshot {
            address: pawn,
            health,
            life_state,
            game_time,
            interrupted,
            melee_threat: false,
            abilities,
        })
    }

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
        let Some(entity_system) = Self::entity_system_ptr(offsets) else {
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
            let parry_success_end = read_f32(
                entity
                    .checked_add(offsets.ability_parry_success_end)
                    .unwrap_or(0),
            )
            .unwrap_or(0.0);
            let melee_state =
                read_u32(entity.checked_add(offsets.ability_melee_state).unwrap_or(0)).unwrap_or(0);
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
                melee_state,
                parry_success_end,
            });
        }
        found
    }

    fn controller_pawn(
        &mut self,
        controller: u64,
        offsets: &Offsets,
        entity_system: u64,
    ) -> Option<u64> {
        let handle = read_u32(
            controller
                .checked_add(offsets.controller_pawn_handle)
                .unwrap_or(0),
        )?;
        let pawn = resolve_entity(handle, offsets, entity_system)?;
        entity_pointer_valid(pawn).then_some(pawn)
    }

    fn abilities_vector(&self, pawn: u64, offsets: &Offsets) -> Option<(u32, u64)> {
        let vector = pawn.checked_add(offsets.abilities_vector).unwrap_or(0);
        let count = read_u32(vector)?;
        let data = read_u64(vector.checked_add(8).unwrap_or(0))?;
        (count > 0 && count <= MAX_ABILITIES && data != 0).then_some((count, data))
    }
    
    fn player_melee_swinging(
        &mut self,
        controller: u64,
        entity_system: u64,
        offsets: &Offsets,
        game_time: f32,
    ) -> bool {
        let Some(pawn) = self.controller_pawn(controller, offsets, entity_system) else {
            self.melee_entities.remove(&controller);
            return false;
        };
        let Some((count, data)) = self.abilities_vector(pawn, offsets) else {
            self.melee_entities.remove(&controller);
            return false;
        };

        if let Some(&handle) = self.melee_entities.get(&controller) {
            if self.melee_handle(handle, offsets, entity_system) {
                return self.melee_swinging(handle, entity_system, offsets, game_time);
            }
            self.melee_entities.remove(&controller);
        }
        for slot_index in 0..count {
            let Some(handle) = read_u32(data + (slot_index as u64) * 4) else {
                continue;
            };
            if self.melee_handle(handle, offsets, entity_system) {
                self.melee_entities.insert(controller, handle);
                return self.melee_swinging(handle, entity_system, offsets, game_time);
            }
        }
        false
    }

    fn melee_handle(&self, handle: u32, offsets: &Offsets, entity_system: u64) -> bool {
        resolve_entity(handle, offsets, entity_system)
            .and_then(|entity| read_u16(entity.checked_add(offsets.ability_slot).unwrap_or(0)))
            .is_some_and(|slot| slot as u8 == offsets.melee_slot)
    }

    fn melee_swinging(&self, handle: u32, entity_system: u64, offsets: &Offsets, game_time: f32) -> bool {
        let Some(entity) = resolve_entity(handle, offsets, entity_system) else {
            return false;
        };
        let state = read_u32(
            entity
                .checked_add(offsets.ability_melee_state)
                .unwrap_or(0),
        )
        .unwrap_or(0);
        if state != 0 {
            return true;
        }
        let swing_time = read_f32(
            entity
                .checked_add(offsets.ability_cooldown_start)
                .unwrap_or(0),
        )
        .unwrap_or(0.0);
        swing_time > 0.0
            && game_time >= swing_time
            && game_time - swing_time <= MELEE_SWING_FRESH_SECONDS
    }

    fn read_players(
        &mut self,
        pawn: u64,
        entity_system: u64,
        offsets: &Offsets,
    ) -> Option<Vec<PlayerSnapshot>> {
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
            });
        }
        Some(players)
    }
}
