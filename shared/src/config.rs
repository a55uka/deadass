use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;

use crate::event::EventKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Pattern {
    Vibrate,
    Pulse,
    Ramp,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataSource {
    Mod,
    #[default]
    Dll,
}

impl DataSource {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "mod" => Some(Self::Mod),
            "dll" => Some(Self::Dll),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Mod => "mod",
            Self::Dll => "dll",
        }
    }
}

pub const DEFAULT_DLL_EVENT_PORT: u16 = 24680;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TriggerKind {
    Kill,
    Death,
    Assist,
    AbilityUsed { slot: u8 },
    AbilityReady { slot: u8 },
    Parry,
    Parried,
    PunchLanded,
    PunchTaken,
    Respawn,
}

impl TriggerKind {
    pub fn family(self) -> TriggerFamily {
        match self {
            Self::Kill => TriggerFamily::Kill,
            Self::Death => TriggerFamily::Death,
            Self::Assist => TriggerFamily::Assist,
            Self::AbilityUsed { .. } => TriggerFamily::AbilityUsed,
            Self::AbilityReady { .. } => TriggerFamily::AbilityReady,
            Self::Parry => TriggerFamily::Parry,
            Self::Parried => TriggerFamily::Parried,
            Self::PunchLanded => TriggerFamily::PunchLanded,
            Self::PunchTaken => TriggerFamily::PunchTaken,
            Self::Respawn => TriggerFamily::Respawn,
        }
    }
}

impl fmt::Display for TriggerKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Kill => write!(f, "kill"),
            Self::Death => write!(f, "death"),
            Self::Assist => write!(f, "assist"),
            Self::Respawn => write!(f, "respawn"),
            Self::AbilityUsed { slot } => write!(f, "ability_used:{slot}"),
            Self::AbilityReady { slot } => write!(f, "ability_ready:{slot}"),
            Self::Parry => write!(f, "parry"),
            Self::Parried => write!(f, "parried"),
            Self::PunchLanded => write!(f, "punch_landed"),
            Self::PunchTaken => write!(f, "punch_taken"),
        }
    }
}

impl FromStr for TriggerKind {
    type Err = String;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        match raw {
            "kill" => Ok(Self::Kill),
            "death" => Ok(Self::Death),
            "assist" => Ok(Self::Assist),
            "respawn" => Ok(Self::Respawn),
            "parry" => Ok(Self::Parry),
            "parried" => Ok(Self::Parried),
            "punch_landed" => Ok(Self::PunchLanded),
            "punch_taken" => Ok(Self::PunchTaken),
            _ => {
                let (name, slot) = raw.split_once(':').ok_or_else(|| unknown_trigger(raw))?;
                let slot: u8 = slot.parse().map_err(|_| unknown_trigger(raw))?;
                match name {
                    "ability_used" => Ok(Self::AbilityUsed { slot }),
                    "ability_ready" => Ok(Self::AbilityReady { slot }),
                    _ => Err(unknown_trigger(raw)),
                }
            }
        }
    }
}

fn unknown_trigger(raw: &str) -> String {
    format!("unknown trigger {raw}")
}

impl From<EventKind> for TriggerKind {
    fn from(kind: EventKind) -> Self {
        match kind {
            EventKind::Kill => Self::Kill,
            EventKind::Death => Self::Death,
            EventKind::Assist => Self::Assist,
            EventKind::AbilityUsed { slot } => Self::AbilityUsed { slot },
            EventKind::AbilityReady { slot } => Self::AbilityReady { slot },
            EventKind::Parry => Self::Parry,
            EventKind::Parried => Self::Parried,
            EventKind::PunchLanded => Self::PunchLanded,
            EventKind::PunchTaken => Self::PunchTaken,
            EventKind::Respawn => Self::Respawn,
        }
    }
}

impl Serialize for TriggerKind {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for TriggerKind {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct TriggerVisitor;

        impl serde::de::Visitor<'_> for TriggerVisitor {
            type Value = TriggerKind;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a trigger key like \"kill\" or \"ability_used:0\"")
            }

            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<TriggerKind, E> {
                value.parse().map_err(E::custom)
            }
        }

        deserializer.deserialize_str(TriggerVisitor)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriggerFamily {
    Kill,
    Death,
    Assist,
    AbilityUsed,
    AbilityReady,
    Parry,
    Parried,
    PunchLanded,
    PunchTaken,
    Respawn,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShockConfig {
    pub enabled: bool,
    /// 1-100, clamped to the OpenShock API range.
    pub intensity: u8,
    /// 300-30000 ms, clamped to the OpenShock API range.
    pub duration_ms: u64,
    /// OpenShock control type (shock, vibrate, or sound).
    pub kind: ShockKind,
    /// Which roster shockers fire, by id. `None` (never touched) means every
    /// roster shocker; an explicit empty list means none.
    #[serde(default)]
    pub shocker_ids: Option<Vec<String>>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShockKind {
    #[default]
    Shock,
    Vibrate,
    Sound,
}

impl ShockConfig {
    pub fn clamped(&self) -> Self {
        Self {
            enabled: self.enabled,
            intensity: self.intensity.clamp(1, 100),
            duration_ms: self.duration_ms.clamp(300, 30_000),
            kind: self.kind,
            shocker_ids: self.shocker_ids.clone(),
        }
    }

    pub fn target_ids(&self, roster: &[Shocker]) -> Vec<String> {
        match &self.shocker_ids {
            None => roster
                .iter()
                .map(|shocker| shocker.id.trim().to_string())
                .filter(|id| !id.is_empty())
                .collect(),
            Some(ids) => filter_shocker_ids(ids, roster),
        }
    }
}

pub fn filter_shocker_ids(ids: &[String], roster: &[Shocker]) -> Vec<String> {
    let mut selected = Vec::new();
    for id in ids {
        let id = id.trim();
        if id.is_empty() || selected.iter().any(|seen| seen == id) {
            continue;
        }
        if roster.iter().any(|shocker| shocker.id.trim() == id) {
            selected.push(id.to_string());
        }
    }
    selected
}

impl Default for ShockConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            intensity: 20,
            duration_ms: 300,
            kind: ShockKind::Shock,
            shocker_ids: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TriggerConfig {
    pub enabled: bool,
    pub strength: f64,
    pub duration_ms: u64,
    pub retrigger_cooldown_ms: u64,
    pub pattern: Pattern,
    #[serde(default)]
    pub shock: Option<ShockConfig>,
    #[serde(default)]
    pub vibrate_devices: Option<Vec<String>>,
}

impl TriggerConfig {
    fn preset(strength: f64, duration_ms: u64) -> Self {
        Self {
            strength,
            duration_ms,
            ..Self::default()
        }
    }

    pub fn scaled_strength(&self, master_gain: f64, cap: f64) -> f64 {
        (self.strength * master_gain).clamp(0.0, cap)
    }
}

impl Default for TriggerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            strength: 0.6,
            duration_ms: 800,
            retrigger_cooldown_ms: 500,
            pattern: Pattern::Vibrate,
            shock: None,
            vibrate_devices: None,
        }
    }
}

pub fn resolve_vibrate_targets(
    selected: Option<&[String]>,
    connected: &[String],
) -> Vec<String> {
    match selected {
        None => connected.to_vec(),
        Some(picks) => connected
            .iter()
            .filter(|name| picks.iter().any(|pick| pick.trim() == *name))
            .cloned()
            .collect(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Shocker {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpenShockConfig {
    pub enabled: bool,
    /// Open-Shock-Token header value.
    pub api_token: String,
    /// Base URL without trailing slash; https://api.openshock.app by default.
    pub base_url: String,
    /// Legacy single-shocker field; migrated into `shockers` on load.
    #[serde(default)]
    pub shocker_id: String,
    /// Every shocker triggers may target.
    #[serde(default)]
    pub shockers: Vec<Shocker>,
}

impl Default for OpenShockConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            api_token: String::new(),
            base_url: String::from("https://api.openshock.app"),
            shocker_id: String::new(),
            shockers: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppConfig {
    pub master_gain: f64,
    pub max_strength_cap: f64,
    pub mute_while_dead: bool,
    pub mod_http_port: u16,
    pub buttplug_ws_url: String,
    #[serde(default)]
    pub debug_logging: bool,
    #[serde(default)]
    pub openshock: OpenShockConfig,
    #[serde(default)]
    pub data_source: DataSource,
    #[serde(default = "default_dll_event_port")]
    pub dll_event_port: u16,
    /// Where the companion looks for deadass-dll.dll when injecting;
    /// empty/None means "next to the running executable".
    #[serde(default)]
    pub dll_path: Option<String>,
    pub triggers: HashMap<TriggerKind, TriggerConfig>,
}

fn default_dll_event_port() -> u16 {
    DEFAULT_DLL_EVENT_PORT
}

impl AppConfig {
    pub fn normalized(mut self) -> Self {
        let legacy = self.openshock.shocker_id.trim().to_string();
        if self.openshock.shockers.is_empty() && !legacy.is_empty() {
            self.openshock.shockers.push(Shocker {
                id: legacy,
                name: String::from("Shocker"),
            });
        }
        self.openshock.shocker_id.clear();
        self
    }

    pub fn trigger(&self, kind: TriggerKind) -> TriggerConfig {
        self.triggers
            .get(&kind)
            .cloned()
            .or_else(|| {
                self.triggers
                    .iter()
                    .find(|(key, _)| key.family() == kind.family())
                    .map(|(_, rule)| (*rule).clone())
            })
            .unwrap_or_default()
    }

    fn default_triggers() -> HashMap<TriggerKind, TriggerConfig> {
        let mut triggers = HashMap::new();
        triggers.insert(TriggerKind::Kill, TriggerConfig::preset(0.7, 900));
        triggers.insert(TriggerKind::Death, TriggerConfig::preset(0.4, 1500));
        triggers.insert(TriggerKind::Assist, TriggerConfig::preset(0.5, 700));
        triggers.insert(TriggerKind::Respawn, TriggerConfig::preset(0.3, 500));
        triggers.insert(TriggerKind::Parry, TriggerConfig::preset(0.7, 250));
        triggers.insert(TriggerKind::Parried, TriggerConfig::preset(0.5, 400));
        triggers.insert(TriggerKind::PunchLanded, TriggerConfig::preset(0.45, 200));
        triggers.insert(TriggerKind::PunchTaken, TriggerConfig::preset(0.4, 200));
        for slot in 0..4 {
            triggers.insert(
                TriggerKind::AbilityUsed { slot },
                TriggerConfig::preset(0.55, 400),
            );
            triggers.insert(
                TriggerKind::AbilityReady { slot },
                TriggerConfig::preset(0.35, 300),
            );
        }
        triggers
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            master_gain: 1.0,
            max_strength_cap: 1.0,
            mute_while_dead: false,
            mod_http_port: 24681,
            buttplug_ws_url: String::from("ws://127.0.0.1:12345"),
            debug_logging: false,
            openshock: OpenShockConfig::default(),
            data_source: DataSource::default(),
            dll_event_port: DEFAULT_DLL_EVENT_PORT,
            dll_path: None,
            triggers: Self::default_triggers(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_logging_defaults_to_off() {
        assert!(!AppConfig::default().debug_logging);
    }

    #[test]
    fn legacy_shocker_migrates_into_roster() {
        let mut config = AppConfig::default();
        config.openshock.shocker_id = String::from("abc123");
        let rule = config
            .triggers
            .get_mut(&TriggerKind::Kill)
            .expect("kill trigger exists");
        rule.shock = Some(ShockConfig {
            enabled: true,
            ..ShockConfig::default()
        });

        let config = config.normalized();
        assert_eq!(config.openshock.shocker_id, "");
        assert_eq!(
            config.openshock.shockers,
            vec![Shocker {
                id: String::from("abc123"),
                name: String::from("Shocker"),
            }]
        );
        let kill = config.triggers.get(&TriggerKind::Kill).unwrap();
        let kill_shock = kill.shock.as_ref().unwrap();
        assert!(kill_shock.shocker_ids.is_none());
        assert_eq!(kill_shock.target_ids(&config.openshock.shockers), ["abc123"]);
    }

    #[test]
    fn shock_selection_filters_to_roster_and_dedupes() {
        let roster = vec![
            Shocker {
                id: String::from("a"),
                name: String::from("Left"),
            },
            Shocker {
                id: String::from("b"),
                name: String::from("Right"),
            },
        ];
        let shock = ShockConfig {
            shocker_ids: Some(vec![
                String::from(" b "),
                String::from("a"),
                String::from("b"),
                String::from("gone"),
                String::from(""),
            ]),
            ..ShockConfig::default()
        };
        assert_eq!(shock.target_ids(&roster), ["b", "a"]);
    }

    #[test]
    fn filter_skips_blank_and_duplicate_ids() {
        let roster = vec![Shocker {
            id: String::from("a"),
            name: String::from("Left"),
        }];
        let ids = vec![String::from("a"), String::from(" "), String::from("a")];
        assert_eq!(filter_shocker_ids(&ids, &roster), ["a"]);
        assert!(filter_shocker_ids(&ids, &[]).is_empty());
    }

    #[test]
    fn empty_shock_selection_targets_whole_roster() {
        let roster = vec![
            Shocker {
                id: String::from(" a "),
                name: String::from("Left"),
            },
            Shocker {
                id: String::from("b"),
                name: String::from("Right"),
            },
            Shocker {
                id: String::from("   "),
                name: String::from("blank"),
            },
        ];
        let shock = ShockConfig::default();
        assert_eq!(shock.target_ids(&roster), ["a", "b"]);
        let shock = ShockConfig {
            shocker_ids: Some(vec![String::from("b")]),
            ..ShockConfig::default()
        };
        assert_eq!(shock.target_ids(&roster), ["b"]);
        let shock = ShockConfig {
            shocker_ids: Some(vec![String::from("gone")]),
            ..ShockConfig::default()
        };
        assert!(shock.target_ids(&roster).is_empty());
        let shock = ShockConfig {
            shocker_ids: Some(vec![]),
            ..ShockConfig::default()
        };
        assert!(shock.target_ids(&roster).is_empty());
    }

    #[test]
    fn vibrate_selection_resolves_against_connected_toys() {
        let connected = vec![String::from("Loonie"), String::from("Edge")];
        assert_eq!(resolve_vibrate_targets(None, &connected), connected);
        assert_eq!(
            resolve_vibrate_targets(Some(&[String::from("Edge")]), &connected),
            ["Edge"]
        );
        assert!(
            resolve_vibrate_targets(Some(&[String::from("Ghost")]), &connected).is_empty()
        );
        assert!(resolve_vibrate_targets(Some(&[]), &connected).is_empty());
    }

    #[test]
    fn shock_config_none_ids_round_trips_as_untouched() {
        let shock = ShockConfig::default();
        assert!(shock.shocker_ids.is_none());
        let rendered = toml::to_string(&shock).expect("shock serializes");
        let parsed: ShockConfig = toml::from_str(&rendered).expect("round trips");
        assert!(parsed.shocker_ids.is_none(), "None means untouched/all");
        let explicit = ShockConfig {
            shocker_ids: Some(vec![String::from("a")]),
            ..shock
        };
        let rendered = toml::to_string(&explicit).expect("serializes");
        let parsed: ShockConfig = toml::from_str(&rendered).expect("round trips");
        assert_eq!(parsed.shocker_ids, Some(vec![String::from("a")]));
    }

    #[test]
    fn legacy_config_without_debug_logging_still_parses_as_off() {
        let rendered = toml::to_string_pretty(&AppConfig::default()).expect("default serializes");
        assert!(rendered.contains("debug_logging"));
        let without_flag: String = rendered
            .lines()
            .filter(|line| !line.trim_start().starts_with("debug_logging"))
            .collect::<Vec<_>>()
            .join("\n");
        let parsed: AppConfig =
            toml::from_str(&without_flag).expect("legacy config without flag parses");
        assert!(!parsed.debug_logging);
    }

    #[test]
    fn config_with_removed_dev_menu_key_still_parses() {
        // An old config file that still carries dev_menu = true.
        let mut rendered = toml::to_string_pretty(&AppConfig::default()).expect("default serializes");
        rendered.push_str("dev_menu = true\n");
        let parsed: AppConfig = toml::from_str(&rendered).expect("unknown keys are ignored");
        assert_eq!(parsed.master_gain, AppConfig::default().master_gain);
    }

    #[test]
    fn trigger_key_round_trips_through_parse() {
        for kind in [
            TriggerKind::Kill,
            TriggerKind::AbilityUsed { slot: 2 },
            TriggerKind::AbilityReady { slot: 0 },
        ] {
            assert_eq!(kind.to_string().parse(), Ok(kind));
        }
    }

    #[test]
    fn unknown_trigger_key_fails_to_parse() {
        assert!("nonsense".parse::<TriggerKind>().is_err());
        assert!("ability_used:x".parse::<TriggerKind>().is_err());
    }

    #[test]
    fn exact_slot_wins_over_family_fallback() {
        let mut config = AppConfig::default();
        config.triggers.insert(
            TriggerKind::AbilityUsed { slot: 1 },
            TriggerConfig::preset(0.9, 100),
        );
        assert_eq!(
            config
                .trigger(TriggerKind::AbilityUsed { slot: 1 })
                .strength,
            0.9
        );
    }
}

#[cfg(test)]
mod shock_tests {
    use super::*;

    #[test]
    fn legacy_trigger_config_without_shock_still_parses() {
        let rendered = r#"
            enabled = true
            strength = 0.7
            duration_ms = 900
            retrigger_cooldown_ms = 500
            pattern = "vibrate"
        "#;
        let rule: TriggerConfig = toml::from_str(rendered).expect("legacy config parses");
        assert!(rule.shock.is_none());
    }

    #[test]
    fn shock_config_round_trips_through_toml() {
        let rule = TriggerConfig {
            shock: Some(ShockConfig {
                enabled: true,
                intensity: 35,
                duration_ms: 600,
                kind: ShockKind::Vibrate,
                shocker_ids: Some(vec![String::from("abc"), String::from("def")]),
            }),
            ..TriggerConfig::default()
        };
        let rendered = toml::to_string(&rule).expect("serializes");
        let parsed: TriggerConfig = toml::from_str(&rendered).expect("round trips");
        let shock = parsed.shock.expect("shock preserved");
        assert!(shock.enabled);
        assert_eq!(shock.intensity, 35);
        assert_eq!(shock.duration_ms, 600);
        assert_eq!(shock.kind, ShockKind::Vibrate);
        assert_eq!(shock.shocker_ids, Some(vec![String::from("abc"), String::from("def")]));
    }
}
