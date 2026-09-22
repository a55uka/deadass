#[cfg(windows)]
mod runtime;

#[cfg(windows)]
pub use runtime::apply_schema;

use super::offsets::Offsets;

type Setter = fn(&mut Offsets, u64);

const DIRECT_FIELDS: &[(&str, &str, Setter)] = &[
    ("C_CitadelPlayerPawn", "m_iHealth", |o, v| o.pawn_health = v),
    ("C_CitadelPlayerPawn", "m_iMaxHealth", |o, v| {
        o.pawn_max_health = v
    }),
    ("C_CitadelPlayerPawn", "m_lifeState", |o, v| {
        o.pawn_life_state = v
    }),
    ("C_CitadelPlayerPawn", "m_iTeamNum", |o, v| o.pawn_team = v),
    ("C_CitadelPlayerPawn", "m_flSimulationTime", |o, v| {
        o.pawn_sim_time = v
    }),
    ("C_CitadelPlayerPawn", "m_hController", |o, v| {
        o.pawn_controller_handle = v
    }),
    (
        "C_CitadelPlayerPawn",
        "m_CCitadelAbilityComponent",
        |o, v| o.pawn_ability_component = v,
    ),
    ("C_CitadelBaseAbility", "m_bChanneling", |o, v| {
        o.ability_channeling = v
    }),
    ("C_CitadelBaseAbility", "m_flCooldownStart", |o, v| {
        o.ability_cooldown_start = v
    }),
    ("C_CitadelBaseAbility", "m_flCooldownEnd", |o, v| {
        o.ability_cooldown_end = v
    }),
    ("C_CitadelBaseAbility", "m_eAbilitySlot", |o, v| {
        o.ability_slot = v
    }),
    ("C_CitadelBaseAbility", "m_iRemainingCharges", |o, v| {
        o.ability_charges = v
    }),
    (
        "CCitadel_Ability_MeleeParry",
        "m_flParryStartTime",
        |o, v| o.ability_parry_start = v,
    ),
    ("CCitadel_Ability_MeleeParry", "m_bAttackParried", |o, v| {
        o.ability_attack_parried = v
    }),
    (
        "CCitadel_Ability_MeleeParry",
        "m_flParrySuccessEndTime",
        |o, v| o.ability_parry_success_end = v,
    ),
    (
        "CCitadel_Ability_HoldMelee",
        "m_eCurrentAttackState",
        |o, v| o.ability_melee_state = v,
    ),
    (
        "CCitadel_Ability_HoldMelee",
        "m_nLightChainCount",
        |o, v| o.ability_melee_chain = v,
    ),
    ("CCitadelPlayerController", "m_PlayerDataGlobal", |o, v| {
        o.controller_player_data = v
    }),
    ("CCitadelPlayerController", "m_hPawn", |o, v| {
        o.controller_pawn_handle = v
    }),
    ("PlayerDataGlobal_t", "m_nHeroID", |o, v| {
        o.player_hero_id = v
    }),
    ("PlayerDataGlobal_t", "m_iHealth", |o, v| {
        o.player_health = v
    }),
    ("PlayerDataGlobal_t", "m_iPlayerKills", |o, v| {
        o.player_kills = v
    }),
    ("PlayerDataGlobal_t", "m_iPlayerAssists", |o, v| {
        o.player_assists = v
    }),
    ("PlayerDataGlobal_t", "m_iDeaths", |o, v| {
        o.player_deaths = v
    }),
    ("PlayerDataGlobal_t", "m_iKillStreak", |o, v| {
        o.player_kill_streak = v
    }),
    ("PlayerDataGlobal_t", "m_bAlive", |o, v| o.player_alive = v),
];

const MAX_FIELD_OFFSET: u32 = 0x8000;

/// Overwrite offset slots from `lookup(class, field)`. Composite offsets
/// (pawn + embedded component + the component's inner field) are computed
/// from their parts. Returns how many offsets were applied; a lookup miss
/// keeps the existing value
pub fn apply_from_schema(
    offsets: &mut Offsets,
    mut lookup: impl FnMut(&str, &str) -> Option<u32>,
) -> usize {
    let mut applied = 0;
    for &(class, field, setter) in DIRECT_FIELDS {
        if let Some(value) = lookup(class, field)
            && value < MAX_FIELD_OFFSET
        {
            setter(offsets, value as u64);
            applied += 1;
        }
    }

    let component_base = offsets.pawn_ability_component;
    if let Some(vec_abilities) = lookup("CCitadelAbilityComponent", "m_vecAbilities")
        && vec_abilities < MAX_FIELD_OFFSET
    {
        offsets.abilities_vector = component_base + vec_abilities as u64;
        applied += 1;
    }
    if let Some(interrupt) = lookup("CCitadelAbilityComponent", "m_bInInterruptState")
        && interrupt < MAX_FIELD_OFFSET
    {
        offsets.pawn_interrupt_state = component_base + interrupt as u64;
        applied += 1;
    }
    if let (Some(damage), Some(last_time)) = (
        lookup("C_CitadelPlayerPawn", "m_sPlayerDamageTaken"),
        lookup("CCitadelRecentDamage", "m_flLastDamageTime"),
    ) && let Some(sum) = damage.checked_add(last_time)
        && sum < MAX_FIELD_OFFSET
    {
        offsets.pawn_damage_taken_time = sum as u64;
        applied += 1;
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
        apply_from_schema(&mut offsets, |class, field| match (class, field) {
            ("C_CitadelPlayerPawn", "m_iHealth") => Some(0x354),
            ("C_CitadelPlayerPawn", "m_CCitadelAbilityComponent") => Some(0x14D0),
            ("CCitadelAbilityComponent", "m_vecAbilities") => Some(0x68),
            ("CCitadelAbilityComponent", "m_bInInterruptState") => Some(0xE4),
            ("C_CitadelPlayerPawn", "m_sPlayerDamageTaken") => Some(0x1480),
            ("CCitadelRecentDamage", "m_flLastDamageTime") => Some(0x8),
            _ => None,
        });
        assert_eq!(offsets.pawn_health, 0x354);
        assert_eq!(offsets.abilities_vector, 0x14D0 + 0x68);
        assert_eq!(offsets.pawn_interrupt_state, 0x14D0 + 0xE4);
        assert_eq!(offsets.pawn_damage_taken_time, 0x1480 + 0x8);
        assert_eq!(offsets.pawn_max_health, Offsets::default().pawn_max_health);
    }

    #[test]
    fn implausible_schema_offsets_are_rejected() {
        let mut offsets = Offsets::default();
        apply_from_schema(&mut offsets, |_, _| Some(0xDEAD_BEEF));
        assert_eq!(offsets.pawn_health, Offsets::default().pawn_health);
    }
}
