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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TriggerKind {
    Kill,
    Death,
    Assist,
    AbilityUsed { slot: u8 },
    AbilityReady { slot: u8 },
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
    Respawn,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TriggerConfig {
    pub enabled: bool,
    pub strength: f64,
    pub duration_ms: u64,
    pub retrigger_cooldown_ms: u64,
    pub pattern: Pattern,
}

impl TriggerConfig {
    fn preset(strength: f64, duration_ms: u64) -> Self {
        Self {
            strength,
            duration_ms,
            ..Self::default()
        }
    }

    pub fn scaled_strength(self, master_gain: f64, cap: f64) -> f64 {
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
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub master_gain: f64,
    pub max_strength_cap: f64,
    pub mute_while_dead: bool,
    pub mod_http_port: u16,
    pub buttplug_ws_url: String,
    #[serde(default)]
    pub debug_logging: bool,
    pub triggers: HashMap<TriggerKind, TriggerConfig>,
}

impl AppConfig {
    pub fn trigger(&self, kind: TriggerKind) -> TriggerConfig {
        self.triggers
            .get(&kind)
            .copied()
            .or_else(|| {
                self.triggers
                    .iter()
                    .find(|(key, _)| key.family() == kind.family())
                    .map(|(_, rule)| *rule)
            })
            .unwrap_or_default()
    }

    fn default_triggers() -> HashMap<TriggerKind, TriggerConfig> {
        let mut triggers = HashMap::new();
        triggers.insert(TriggerKind::Kill, TriggerConfig::preset(0.7, 900));
        triggers.insert(TriggerKind::Death, TriggerConfig::preset(0.4, 1500));
        triggers.insert(TriggerKind::Assist, TriggerConfig::preset(0.5, 700));
        triggers.insert(TriggerKind::Respawn, TriggerConfig::preset(0.3, 500));
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
    fn config_without_debug_logging_key_still_parses_as_off() {
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
