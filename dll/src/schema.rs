//! Runtime schema resolution — the patch-proof source for FIELD offsets.
//!
//! Source 2 keeps every networked class's field layout in an in-memory schema
//! (the same data dezlock-dump exports). Instead of baking field offsets that
//! rot with every patch, the DLL asks the game directly: given a class name
//! and field name, the schema system returns the current offset. Only struct
//! GLOBALS still need signatures (see `resolve`); fields come from here.
//!
//! Access path (interfaces documented by dezlock-dump):
//!   schemasystem.dll!CreateInterface("SchemaSystem_001")   -> CSchemaSystem
//!   CSchemaSystem vtable[13]                               -> type scope by
//!   module name ("client.dll")
//!   type scope vtable[2]                                   -> CSchemaClassInfo
//!   CSchemaClassInfo: field_count i16 @ +0x1C, fields * @ +0x28
//!   field entries: 0x20 stride, name char* @ +0x00, offset i32 @ +0x10
//!
//! The class info's field list is FLATTENED — inherited fields included — so
//! e.g. C_CitadelPlayerPawn's list contains m_iHealth from CBaseEntity at
//! its final offset.
//!
//! Precedence: baked defaults -> this module -> deadass-offsets.toml (an
//! explicit toml pin always wins).

use super::offsets::Offsets;

/// Which schema classes and fields feed which offset slot. Pure data, so the
/// application logic is unit-testable with a fake lookup.
type Setter = fn(&mut Offsets, u64);

const DIRECT_FIELDS: &[(&str, &str, Setter)] = &[
    ("C_CitadelPlayerPawn", "m_iHealth", |o, v| o.pawn_health = v),
    ("C_CitadelPlayerPawn", "m_iMaxHealth", |o, v| o.pawn_max_health = v),
    ("C_CitadelPlayerPawn", "m_lifeState", |o, v| o.pawn_life_state = v),
    ("C_CitadelPlayerPawn", "m_iTeamNum", |o, v| o.pawn_team = v),
    ("C_CitadelPlayerPawn", "m_flSimulationTime", |o, v| o.pawn_sim_time = v),
    ("C_CitadelPlayerPawn", "m_hController", |o, v| o.pawn_controller_handle = v),
    ("C_CitadelPlayerPawn", "m_CCitadelAbilityComponent", |o, v| {
        o.pawn_ability_component = v
    }),
    ("C_CitadelBaseAbility", "m_bChanneling", |o, v| o.ability_channeling = v),
    ("C_CitadelBaseAbility", "m_flCooldownStart", |o, v| {
        o.ability_cooldown_start = v
    }),
    ("C_CitadelBaseAbility", "m_flCooldownEnd", |o, v| o.ability_cooldown_end = v),
    ("C_CitadelBaseAbility", "m_eAbilitySlot", |o, v| o.ability_slot = v),
    ("C_CitadelBaseAbility", "m_iRemainingCharges", |o, v| o.ability_charges = v),
    ("CCitadel_Ability_MeleeParry", "m_flParryStartTime", |o, v| {
        o.ability_parry_start = v
    }),
    ("CCitadel_Ability_MeleeParry", "m_bAttackParried", |o, v| {
        o.ability_attack_parried = v
    }),
    ("CCitadel_Ability_MeleeParry", "m_flParrySuccessEndTime", |o, v| {
        o.ability_parry_success_end = v
    }),
    ("CCitadel_Ability_HoldMelee", "m_eCurrentAttackState", |o, v| {
        o.ability_melee_state = v
    }),
    ("CCitadel_Ability_HoldMelee", "m_nLightChainCount", |o, v| {
        o.ability_melee_chain = v
    }),
    ("CCitadelPlayerController", "m_PlayerDataGlobal", |o, v| {
        o.controller_player_data = v
    }),
    ("CCitadelPlayerController", "m_hPawn", |o, v| o.controller_pawn_handle = v),
    ("PlayerDataGlobal_t", "m_nHeroID", |o, v| o.player_hero_id = v),
    ("PlayerDataGlobal_t", "m_iHealth", |o, v| o.player_health = v),
    ("PlayerDataGlobal_t", "m_iPlayerKills", |o, v| o.player_kills = v),
    ("PlayerDataGlobal_t", "m_iPlayerAssists", |o, v| o.player_assists = v),
    ("PlayerDataGlobal_t", "m_iDeaths", |o, v| o.player_deaths = v),
    ("PlayerDataGlobal_t", "m_iKillStreak", |o, v| o.player_kill_streak = v),
    ("PlayerDataGlobal_t", "m_bAlive", |o, v| o.player_alive = v),
];

/// Field offsets live inside real structs; anything beyond this is garbage.
const MAX_FIELD_OFFSET: u32 = 0x8000;

/// Overwrite offset slots from `lookup(class, field)`. Composite offsets
/// (pawn + embedded component + the component's inner field) are computed
/// from their parts. Returns how many offsets were applied; a lookup miss
/// keeps the existing value.
pub fn apply_from_schema(
    offsets: &mut Offsets,
    mut lookup: impl FnMut(&str, &str) -> Option<u32>,
) -> usize {
    let mut applied = 0;
    for &(class, field, setter) in DIRECT_FIELDS {
        if let Some(value) = lookup(class, field) {
            if value < MAX_FIELD_OFFSET {
                setter(offsets, value as u64);
                applied += 1;
            }
        }
    }

    // Composites: pawn + embedded ability component + inner fields.
    let component_base = offsets.pawn_ability_component;
    if let Some(vec_abilities) = lookup("CCitadelAbilityComponent", "m_vecAbilities") {
        if vec_abilities < MAX_FIELD_OFFSET {
            offsets.abilities_vector = component_base + vec_abilities as u64;
            applied += 1;
        }
    }
    if let Some(interrupt) = lookup("CCitadelAbilityComponent", "m_bInInterruptState") {
        if interrupt < MAX_FIELD_OFFSET {
            offsets.pawn_interrupt_state = component_base + interrupt as u64;
            applied += 1;
        }
    }
    // Damage-taken timestamp: pawn + m_sPlayerDamageTaken + inner time field.
    if let (Some(damage), Some(last_time)) = (
        lookup("C_CitadelPlayerPawn", "m_sPlayerDamageTaken"),
        lookup("CCitadelRecentDamage", "m_flLastDamageTime"),
    ) {
        if damage + last_time < MAX_FIELD_OFFSET {
            offsets.pawn_damage_taken_time = (damage + last_time) as u64;
            applied += 1;
        }
    }
    applied
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_schema_field_entry_is_unique() {
        let mut seen = std::collections::HashSet::new();
        for &(class, field, _) in DIRECT_FIELDS {
            assert!(seen.insert((class, field)), "duplicate {class}.{field}");
        }
    }

    #[test]
    fn apply_resolves_direct_and_composite_offsets() {
        let mut offsets = Offsets::default();
        apply_from_schema(&mut offsets, |class, field| {
            match (class, field) {
                ("C_CitadelPlayerPawn", "m_iHealth") => Some(0x354),
                ("C_CitadelPlayerPawn", "m_CCitadelAbilityComponent") => Some(0x14D0),
                ("CCitadelAbilityComponent", "m_vecAbilities") => Some(0x68),
                ("CCitadelAbilityComponent", "m_bInInterruptState") => Some(0xE4),
                ("C_CitadelPlayerPawn", "m_sPlayerDamageTaken") => Some(0x1480),
                ("CCitadelRecentDamage", "m_flLastDamageTime") => Some(0x8),
                _ => None,
            }
        });
        assert_eq!(offsets.pawn_health, 0x354);
        assert_eq!(offsets.abilities_vector, 0x14D0 + 0x68);
        assert_eq!(offsets.pawn_interrupt_state, 0x14D0 + 0xE4);
        assert_eq!(offsets.pawn_damage_taken_time, 0x1480 + 0x8);
        // Misses keep the baked values.
        assert_eq!(offsets.pawn_max_health, Offsets::default().pawn_max_health);
    }

    #[test]
    fn implausible_schema_offsets_are_rejected() {
        let mut offsets = Offsets::default();
        apply_from_schema(&mut offsets, |_, _| Some(0xDEAD_BEEF));
        assert_eq!(offsets.pawn_health, Offsets::default().pawn_health);
    }
}

// ---------------------------------------------------------------------------
// Windows runtime: the real lookup backed by the game's schema system.
// ---------------------------------------------------------------------------
#[cfg(windows)]
pub use windows_impl::apply_schema;

#[cfg(windows)]
mod windows_impl {
    use super::*;
    use std::collections::HashMap;
    use std::ffi::c_void;

    use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleA, GetProcAddress};

    type CreateInterfaceFn =
        unsafe extern "system" fn(*const u8, *mut i32) -> *mut c_void;
    type FindTypeScopeFn =
        unsafe extern "system" fn(*mut c_void, *const u8, *mut c_void) -> *mut c_void;
    type FindDeclaredClassFn =
        unsafe extern "system" fn(*mut c_void, *mut *mut c_void, *const u8);

    /// The game's schema system, plus a per-class field cache. All reads are
    /// page-checked; the two vtable calls use the indices documented by
    /// dezlock-dump and are only made on pointers that validated.
    struct SchemaRuntime {
        scope: *mut c_void,
        find_class: FindDeclaredClassFn,
        classes: HashMap<String, HashMap<String, u32>>,
    }

    fn page_checked(address: u64, len: usize) -> Option<&'static [u8]> {
        if !super::super::game::pages_committed(address, len) {
            return None;
        }
        Some(unsafe { std::slice::from_raw_parts(address as *const u8, len) })
    }

    fn read_u16(address: u64) -> Option<u16> {
        let bytes = page_checked(address, 2)?;
        Some(u16::from_le_bytes(bytes.try_into().ok()?))
    }

    fn read_u32(address: u64) -> Option<u32> {
        let bytes = page_checked(address, 4)?;
        Some(u32::from_le_bytes(bytes.try_into().ok()?))
    }

    fn read_u64(address: u64) -> Option<u64> {
        let bytes = page_checked(address, 8)?;
        Some(u64::from_le_bytes(bytes.try_into().ok()?))
    }

    fn read_c_string(address: u64) -> Option<String> {
        if address == 0 {
            return None;
        }
        let bytes = page_checked(address, 64)?;
        let end = bytes
            .iter()
            .position(|&byte| byte == 0)
            .unwrap_or(bytes.len());
        Some(String::from_utf8_lossy(&bytes[..end]).into_owned())
    }

    fn wide(name: &str) -> Vec<u8> {
        name.bytes().chain(std::iter::once(0)).collect()
    }

    impl SchemaRuntime {
        /// Attach to the schema system and client.dll's type scope. `None`
        /// until schemasystem.dll AND client.dll are loaded — callers retry.
        fn attach() -> Option<Self> {
            if unsafe { GetModuleHandleA(wide("client.dll").as_ptr()) }.is_null() {
                return None;
            }
            let module = unsafe { GetModuleHandleA(wide("schemasystem.dll").as_ptr()) };
            if module.is_null() {
                return None;
            }
            let factory =
                unsafe { GetProcAddress(module, wide("CreateInterface").as_ptr()) }?;
            let factory: CreateInterfaceFn = unsafe { std::mem::transmute(factory) };
            let system =
                unsafe { factory(wide("SchemaSystem_001").as_ptr(), std::ptr::null_mut()) };
            if system.is_null() {
                return None;
            }

            // CSchemaSystem::FindTypeScope(module_name, nullptr)
            let system_vtable = read_u64(system as u64)?;
            let find_type_scope =
                unsafe { std::mem::transmute::<u64, FindTypeScopeFn>(read_u64(system_vtable + 13 * 8)?) };
            let scope = unsafe {
                find_type_scope(system, wide("client.dll").as_ptr(), std::ptr::null_mut())
            };
            if scope.is_null() {
                return None;
            }

            // Scope::FindDeclaredClass(out, class_name)
            let scope_vtable = read_u64(scope as u64)?;
            let find_class =
                unsafe { std::mem::transmute::<u64, FindDeclaredClassFn>(read_u64(scope_vtable + 2 * 8)?) };

            Some(Self {
                scope,
                find_class,
                classes: HashMap::new(),
            })
        }

        /// A class's flattened (field name -> offset) map, cached on first
        /// use. `None` when the class is unknown or unreadable.
        fn class_fields(&mut self, class: &str) -> Option<&HashMap<String, u32>> {
            if !self.classes.contains_key(class) {
                let fields = self.read_class_fields(class)?;
                self.classes.insert(class.to_string(), fields);
            }
            self.classes.get(class)
        }

        fn read_class_fields(&self, class: &str) -> Option<HashMap<String, u32>> {
            let mut info: *mut c_void = std::ptr::null_mut();
            unsafe { (self.find_class)(self.scope, &mut info, wide(class).as_ptr()) };
            if info.is_null() {
                return None;
            }
            let info = info as u64;
            let field_count = read_u16(info + 0x1C)? as usize;
            let fields_ptr = read_u64(info + 0x28)?;
            if fields_ptr == 0 || field_count == 0 || field_count > 4096 {
                return None;
            }

            let mut fields = HashMap::with_capacity(field_count);
            for index in 0..field_count {
                let entry = fields_ptr + (index as u64) * 0x20;
                let Some(name_ptr) = read_u64(entry) else {
                    continue;
                };
                let Some(name) = read_c_string(name_ptr) else {
                    continue;
                };
                let Some(offset) = read_u32(entry + 0x10) else {
                    continue;
                };
                fields.insert(name, offset);
            }
            Some(fields)
        }
    }

    /// Resolve every schema-backed field offset into `offsets`. True when the
    /// schema system was reachable and at least a few fields resolved (the
    /// caller stops retrying); individual misses keep their baked values.
    pub fn apply_schema(offsets: &mut Offsets) -> bool {
        let Some(mut runtime) = SchemaRuntime::attach() else {
            return false;
        };
        let applied = apply_from_schema(offsets, |class, field| {
            runtime
                .class_fields(class)
                .and_then(|fields| fields.get(field).copied())
        });
        crate::debug_log::always(&format!(
            "schema runtime: {applied} field offsets resolved"
        ));
        applied >= 8
    }
}
