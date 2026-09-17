use serde::Deserialize;

pub const DEFAULT_DLL_PORT: u16 = 24680;
pub const DEFAULT_POLL_INTERVAL_MS: u64 = 33;

pub const OFFSETS_FILE_NAME: &str = "deadass-offsets.toml";
pub const OFFSETS_ENV_VAR: &str = "DEADASS_OFFSETS";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Offsets {
    /// TCP port of the companion's DLL event server.
    pub dll_port: u16,
    /// How often to sample game state, in milliseconds.
    pub poll_interval_ms: u64,
    /// client.dll+ — global `C_CitadelPlayerPawn*` for the local player.
    pub local_pawn_global: u64,
    /// client.dll+ — global `CGameEntitySystem*` used to resolve entity handles.
    pub entity_system_global: u64,
    /// Entity list chunk array sits at entity_system + this offset.
    pub entity_chunk_array: u64,
    /// Entity indices per chunk (Source 2 uses 512).
    pub entity_chunk_size: u64,
    /// Slot stride inside a chunk.
    pub entity_stride: u64,
    /// C_CitadelPlayerPawn field offsets.
    pub pawn_health: u64,
    pub pawn_max_health: u64,
    pub pawn_life_state: u64,
    pub pawn_team: u64,
    /// m_flSimulationTime (GameTime_t) — the pawn's current game clock, used
    /// to decide whether a cooldown has actually expired.
    pub pawn_sim_time: u64,
    /// m_hController (CHandle<CBasePlayerController>) on the pawn.
    pub pawn_controller_handle: u64,
    /// CCitadelAbilityComponent.m_bInInterruptState (pawn + component + 0xE4):
    /// our cast was interrupted — includes getting parried.
    pub pawn_interrupt_state: u64,
    /// CCitadelRecentDamage.m_flLastDamageTime inside m_sPlayerDamageTaken
    /// (pawn + 0x1480 + 0x8): game time when a PLAYER last damaged us.
    pub pawn_damage_taken_time: u64,
    /// Embedded CCitadelAbilityComponent at pawn + this offset.
    pub pawn_ability_component: u64,
    /// m_vecAbilities inside the ability component.
    pub abilities_vector: u64,
    /// C_CitadelBaseAbility field offsets.
    pub ability_channeling: u64,
    pub ability_cooldown_start: u64,
    pub ability_cooldown_end: u64,
    pub ability_slot: u64,
    pub ability_charges: u64,
    /// CCitadel_Ability_MeleeParry fields (meaningful only on the parry
    /// ability entity; read on every ability and gated by freshness).
    pub ability_parry_start: u64,
    pub ability_attack_parried: u64,
    pub ability_parry_success_end: u64,
    /// CCitadel_Ability_HoldMelee melee state (EMeleeHold_AttackState).
    pub ability_melee_state: u64,
    /// CCitadel_Ability_HoldMelee.m_nLightChainCount — increments on landed
    /// light melee hits.
    pub ability_melee_chain: u64,
    /// The weapon-melee ability slot (ESlot_Weapon_Melee).
    pub melee_slot: u8,
    /// Highest ability slot tracked for parry detection (slots beyond the
    /// four hero keys: melee, parry, etc.).
    pub max_ability_slot: u8,
    /// CCitadelPlayerController embedded PlayerDataGlobal offset, plus the
    /// scoreboard fields inside it (all networked for every player).
    pub controller_player_data: u64,
    /// CBasePlayerController.m_hPawn (CHandle to the player's pawn).
    pub controller_pawn_handle: u64,
    pub player_health: u64,
    pub player_hero_id: u64,
    pub player_kills: u64,
    pub player_assists: u64,
    pub player_deaths: u64,
    pub player_kill_streak: u64,
    pub player_alive: u64,
}

impl Default for Offsets {
    fn default() -> Self {
        // Current Deadlock build — dezlock-dump globals live-verified on
        // 2026-09-13: the local pawn global read back health=414/max=882/
        // life_state=0 while the player was alive in a match, and both entity
        // system globals (dezlock's and the pattern-scanned dwEntityList at
        // 0x391EDA8) resolve to the same live instance. Entity list stride is
        // 0x70 in this build (not the classic 0x78).
        Self {
            dll_port: DEFAULT_DLL_PORT,
            poll_interval_ms: DEFAULT_POLL_INTERVAL_MS,
            local_pawn_global: 0x2F14178,
            entity_system_global: 0x30E68A0,
            entity_chunk_array: 0x10,
            entity_chunk_size: 512,
            entity_stride: 0x70,
            pawn_health: 0x354,
            pawn_max_health: 0x350,
            pawn_life_state: 0x35C,
            pawn_team: 0x3F3,
            pawn_sim_time: 0x3C0,
            pawn_controller_handle: 0x10B0,
            pawn_interrupt_state: 0x14D0 + 0xE4,
            pawn_damage_taken_time: 0x1480 + 0x8,
            pawn_ability_component: 0x14D0,
            abilities_vector: 0x1538,
            ability_channeling: 0x740,
            ability_cooldown_start: 0x764,
            ability_cooldown_end: 0x768,
            ability_slot: 0x778,
            ability_charges: 0x780,
            ability_parry_start: 0x11DC,
            ability_attack_parried: 0x11E0,
            ability_parry_success_end: 0x11E4,
            ability_melee_state: 0x12F0,
            ability_melee_chain: 0x130C,
            melee_slot: 22,
            max_ability_slot: 22,
            controller_player_data: 0x8F0,
            controller_pawn_handle: 0x6BC,
            player_health: 0x50,
            player_hero_id: 0x1C,
            player_kills: 0x54,
            player_assists: 0x58,
            player_deaths: 0x5C,
            player_kill_streak: 0x68,
            player_alive: 0x6C,
        }
    }
}

#[derive(Debug, Default, Deserialize)]
struct OffsetsFile {
    dll: Option<DllSection>,
    globals: Option<GlobalsSection>,
    entity_list: Option<EntityListSection>,
    pawn: Option<PawnSection>,
    controller: Option<ControllerSection>,
    ability: Option<AbilitySection>,
}

#[derive(Debug, Deserialize)]
struct DllSection {
    port: Option<u16>,
    poll_interval_ms: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct GlobalsSection {
    local_pawn: Option<Hex>,
    entity_system: Option<Hex>,
}

#[derive(Debug, Deserialize)]
struct EntityListSection {
    chunk_array: Option<Hex>,
    chunk_size: Option<u64>,
    stride: Option<Hex>,
}

#[derive(Debug, Deserialize)]
struct PawnSection {
    health: Option<Hex>,
    max_health: Option<Hex>,
    life_state: Option<Hex>,
    team: Option<Hex>,
    sim_time: Option<Hex>,
    interrupt_state: Option<Hex>,
    damage_taken_time: Option<Hex>,
    controller_handle: Option<Hex>,
    ability_component: Option<Hex>,
    #[serde(rename = "abilities")]
    abilities: Option<Hex>,
}

#[derive(Debug, Deserialize)]
struct ControllerSection {
    player_data: Option<Hex>,
    pawn_handle: Option<Hex>,
    health: Option<Hex>,
    hero_id: Option<Hex>,
    kills: Option<Hex>,
    assists: Option<Hex>,
    deaths: Option<Hex>,
    kill_streak: Option<Hex>,
    alive: Option<Hex>,
}

#[derive(Debug, Deserialize)]
struct AbilitySection {
    channeling: Option<Hex>,
    cooldown_start: Option<Hex>,
    cooldown_end: Option<Hex>,
    slot: Option<Hex>,
    charges: Option<Hex>,
    parry_start: Option<Hex>,
    attack_parried: Option<Hex>,
    parry_success_end: Option<Hex>,
    melee_state: Option<Hex>,
    melee_chain: Option<Hex>,
    melee_slot: Option<u8>,
    max_slot: Option<u8>,
}

/// TOML has no hex integer literals, so offsets are written as strings
/// ("0x2E76FE8") or plain decimal integers; both parse here.
#[derive(Debug)]
struct Hex(u64);

impl<'de> Deserialize<'de> for Hex {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct HexVisitor;

        impl serde::de::Visitor<'_> for HexVisitor {
            type Value = Hex;

            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an integer or \"0x…\" string offset")
            }

            fn visit_u64<E: serde::de::Error>(self, value: u64) -> Result<Self::Value, E> {
                Ok(Hex(value))
            }

            fn visit_i64<E: serde::de::Error>(self, value: i64) -> Result<Self::Value, E> {
                u64::try_from(value).map(Hex).map_err(E::custom)
            }

            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                let trimmed = value.trim();
                let parsed = trimmed
                    .strip_prefix("0x")
                    .or_else(|| trimmed.strip_prefix("0X"))
                    .map_or_else(
                        || trimmed.parse::<u64>(),
                        |digits| u64::from_str_radix(digits, 16),
                    );
                parsed.map(Hex).map_err(E::custom)
            }
        }

        deserializer.deserialize_any(HexVisitor)
    }
}

impl Offsets {
    /// Defaults overlaid with the first override file that exists, in order:
    /// 1. `$DEADASS_OFFSETS`
    /// 2. `deadass-offsets.toml` next to this DLL
    pub fn load() -> Self {
        let mut offsets = Self::default();
        if let Some(path) = override_path() {
            match std::fs::read_to_string(&path) {
                Ok(raw) => {
                    if let Err(error) = offsets.overlay(&raw) {
                        eprintln!("[deadass] bad offsets file {}: {error}", path.display());
                    }
                }
                Err(error) if path.is_file() => {
                    eprintln!("[deadass] unreadable offsets file {}: {error}", path.display());
                }
                Err(_) => {}
            }
        }
        offsets
    }

    fn overlay(&mut self, raw: &str) -> Result<(), String> {
        let file: OffsetsFile = toml::from_str(raw).map_err(|error| error.to_string())?;
        if let Some(dll) = file.dll {
            if let Some(port) = dll.port {
                self.dll_port = port;
            }
            if let Some(interval) = dll.poll_interval_ms {
                self.poll_interval_ms = interval.max(1);
            }
        }
        if let Some(globals) = file.globals {
            if let Some(value) = globals.local_pawn {
                self.local_pawn_global = value.0;
            }
            if let Some(value) = globals.entity_system {
                self.entity_system_global = value.0;
            }
        }
        if let Some(list) = file.entity_list {
            if let Some(value) = list.chunk_array {
                self.entity_chunk_array = value.0;
            }
            if let Some(value) = list.chunk_size {
                self.entity_chunk_size = value.max(1);
            }
            if let Some(value) = list.stride {
                self.entity_stride = value.0;
            }
        }
        if let Some(pawn) = file.pawn {
            if let Some(value) = pawn.health {
                self.pawn_health = value.0;
            }
            if let Some(value) = pawn.max_health {
                self.pawn_max_health = value.0;
            }
            if let Some(value) = pawn.life_state {
                self.pawn_life_state = value.0;
            }
            if let Some(value) = pawn.team {
                self.pawn_team = value.0;
            }
            if let Some(value) = pawn.sim_time {
                self.pawn_sim_time = value.0;
            }
            if let Some(value) = pawn.controller_handle {
                self.pawn_controller_handle = value.0;
            }
            if let Some(value) = pawn.interrupt_state {
                self.pawn_interrupt_state = value.0;
            }
            if let Some(value) = pawn.damage_taken_time {
                self.pawn_damage_taken_time = value.0;
            }
            if let Some(value) = pawn.ability_component {
                self.pawn_ability_component = value.0;
            }
            if let Some(value) = pawn.abilities {
                self.abilities_vector = value.0;
            }
        }
        if let Some(controller) = file.controller {
            if let Some(value) = controller.player_data {
                self.controller_player_data = value.0;
            }
            if let Some(value) = controller.pawn_handle {
                self.controller_pawn_handle = value.0;
            }
            if let Some(value) = controller.health {
                self.player_health = value.0;
            }
            if let Some(value) = controller.hero_id {
                self.player_hero_id = value.0;
            }
            if let Some(value) = controller.kills {
                self.player_kills = value.0;
            }
            if let Some(value) = controller.assists {
                self.player_assists = value.0;
            }
            if let Some(value) = controller.deaths {
                self.player_deaths = value.0;
            }
            if let Some(value) = controller.kill_streak {
                self.player_kill_streak = value.0;
            }
            if let Some(value) = controller.alive {
                self.player_alive = value.0;
            }
        }
        if let Some(ability) = file.ability {
            if let Some(value) = ability.channeling {
                self.ability_channeling = value.0;
            }
            if let Some(value) = ability.cooldown_start {
                self.ability_cooldown_start = value.0;
            }
            if let Some(value) = ability.cooldown_end {
                self.ability_cooldown_end = value.0;
            }
            if let Some(value) = ability.slot {
                self.ability_slot = value.0;
            }
            if let Some(value) = ability.charges {
                self.ability_charges = value.0;
            }
            if let Some(value) = ability.parry_start {
                self.ability_parry_start = value.0;
            }
            if let Some(value) = ability.attack_parried {
                self.ability_attack_parried = value.0;
            }
            if let Some(value) = ability.parry_success_end {
                self.ability_parry_success_end = value.0;
            }
            if let Some(value) = ability.melee_state {
                self.ability_melee_state = value.0;
            }
            if let Some(value) = ability.melee_chain {
                self.ability_melee_chain = value.0;
            }
            if let Some(value) = ability.melee_slot {
                self.melee_slot = value;
            }
            if let Some(value) = ability.max_slot {
                self.max_ability_slot = value;
            }
        }
        Ok(())
    }
}

fn override_path() -> Option<std::path::PathBuf> {
    if let Ok(path) = std::env::var(OFFSETS_ENV_VAR) {
        let path = std::path::PathBuf::from(path);
        if path.is_file() {
            return Some(path);
        }
    }
    dll_directory().map(|dir| dir.join(OFFSETS_FILE_NAME))
}

#[cfg(windows)]
pub(crate) fn dll_directory() -> Option<std::path::PathBuf> {
    use windows_sys::Win32::System::LibraryLoader::{
        GetModuleFileNameW, GetModuleHandleExW, GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS,
        GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
    };

    extern "system" fn pin_module() {}
    let pinned = pin_module as extern "system" fn() as usize;
    let mut module = std::ptr::null_mut();
    let owned = unsafe {
        GetModuleHandleExW(
            GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
            pinned as *const u16,
            &mut module,
        )
    };
    if owned == 0 || module.is_null() {
        return None;
    }
    let mut buffer = [0u16; 512];
    let len = unsafe { GetModuleFileNameW(module, buffer.as_mut_ptr(), buffer.len() as u32) } as usize;
    if len == 0 || len as usize >= buffer.len() {
        return None;
    }
    let path = String::from_utf16_lossy(&buffer[..len]);
    std::path::PathBuf::from(path).parent().map(|parent| parent.to_path_buf())
}

#[cfg(not(windows))]
pub(crate) fn dll_directory() -> Option<std::path::PathBuf> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlay_reads_hex_strings_and_partial_files() {
        let mut offsets = Offsets::default();
        offsets
            .overlay(
                r#"
                [globals]
                local_pawn = "0x1234"

                [pawn]
                health = 720
                "#,
            )
            .expect("overlay parses");
        assert_eq!(offsets.local_pawn_global, 0x1234);
        assert_eq!(offsets.pawn_health, 720);
        assert_eq!(offsets.entity_system_global, Offsets::default().entity_system_global);
    }

    #[test]
    fn hex_visitor_accepts_decimal_and_hex_strings() {
        let value: Hex = serde_json::from_str("\"0xFF\"").unwrap();
        assert_eq!(value.0, 255);
        let value: Hex = serde_json::from_str("16").unwrap();
        assert_eq!(value.0, 16);
        assert!(serde_json::from_str::<Hex>("\"zz\"").is_err());
    }
}
