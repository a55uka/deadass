use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
    pub fn key(self) -> String {
        match self {
            TriggerKind::Kill => "kill".to_owned(),
            TriggerKind::Death => "death".to_owned(),
            TriggerKind::Assist => "assist".to_owned(),
            TriggerKind::Respawn => "respawn".to_owned(),
            TriggerKind::AbilityUsed { slot } => format!("ability_used:{slot}"),
            TriggerKind::AbilityReady { slot } => format!("ability_ready:{slot}"),
        }
    }

    fn parse_key(raw: &str) -> Option<Self> {
        match raw {
            "kill" => Some(TriggerKind::Kill),
            "death" => Some(TriggerKind::Death),
            "assist" => Some(TriggerKind::Assist),
            "respawn" => Some(TriggerKind::Respawn),
            _ => {
                let (name, slot) = raw.split_once(':')?;
                let slot: u8 = slot.parse().ok()?;
                match name {
                    "ability_used" => Some(TriggerKind::AbilityUsed { slot }),
                    "ability_ready" => Some(TriggerKind::AbilityReady { slot }),
                    _ => None,
                }
            }
        }
    }

    pub fn family(&self) -> TriggerFamily {
        match self {
            TriggerKind::Kill => TriggerFamily::Kill,
            TriggerKind::Death => TriggerFamily::Death,
            TriggerKind::Assist => TriggerFamily::Assist,
            TriggerKind::AbilityUsed { .. } => TriggerFamily::AbilityUsed,
            TriggerKind::AbilityReady { .. } => TriggerFamily::AbilityReady,
            TriggerKind::Respawn => TriggerFamily::Respawn,
        }
    }

    pub fn from_event(kind: EventKind) -> Self {
        match kind {
            EventKind::Kill => TriggerKind::Kill,
            EventKind::Death => TriggerKind::Death,
            EventKind::Assist => TriggerKind::Assist,
            EventKind::AbilityUsed { slot } => TriggerKind::AbilityUsed { slot },
            EventKind::AbilityReady { slot } => TriggerKind::AbilityReady { slot },
            EventKind::Respawn => TriggerKind::Respawn,
        }
    }
}

impl Serialize for TriggerKind {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.key())
    }
}

impl<'de> Deserialize<'de> for TriggerKind {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct KeyVisitor;

        impl serde::de::Visitor<'_> for KeyVisitor {
            type Value = TriggerKind;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a trigger key like \"kill\" or \"ability_used:0\"")
            }

            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<TriggerKind, E> {
                TriggerKind::parse_key(value)
                    .ok_or_else(|| E::custom(format!("unknown trigger {value}")))
            }
        }

        deserializer.deserialize_str(KeyVisitor)
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
        if let Some(found) = self.triggers.get(&kind) {
            return *found;
        }
        let family_fallback = self
            .triggers
            .iter()
            .find_map(|(key, value)| (key.family() == kind.family()).then_some(*value));
        family_fallback.unwrap_or_default()
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        let mut triggers = HashMap::new();
        triggers.insert(
            TriggerKind::Kill,
            TriggerConfig {
                strength: 0.7,
                duration_ms: 900,
                ..TriggerConfig::default()
            },
        );
        triggers.insert(
            TriggerKind::Death,
            TriggerConfig {
                strength: 0.4,
                duration_ms: 1500,
                ..TriggerConfig::default()
            },
        );
        triggers.insert(
            TriggerKind::Assist,
            TriggerConfig {
                strength: 0.5,
                duration_ms: 700,
                ..TriggerConfig::default()
            },
        );
        triggers.insert(
            TriggerKind::Respawn,
            TriggerConfig {
                strength: 0.3,
                duration_ms: 500,
                ..TriggerConfig::default()
            },
        );
        for slot in 0..4 {
            triggers.insert(
                TriggerKind::AbilityUsed { slot },
                TriggerConfig {
                    strength: 0.55,
                    duration_ms: 400,
                    ..TriggerConfig::default()
                },
            );
            triggers.insert(
                TriggerKind::AbilityReady { slot },
                TriggerConfig {
                    strength: 0.35,
                    duration_ms: 300,
                    ..TriggerConfig::default()
                },
            );
        }
        Self {
            master_gain: 1.0,
            max_strength_cap: 1.0,
            mute_while_dead: false,
            mod_http_port: 24681,
            buttplug_ws_url: "ws://127.0.0.1:12345".to_string(),
            debug_logging: false,
            triggers,
        }
    }
}

#[cfg(test)]
mod debug_logging_default {
    use super::*;

    #[test]
    fn defaults_to_off() {
        assert!(!AppConfig::default().debug_logging);
    }

    #[test]
    fn missing_key_in_toml_still_parses_as_off() {
        let full = toml::to_string_pretty(&AppConfig::default()).expect("default serializes");
        assert!(full.contains("debug_logging"));
        let stripped: String = full
            .lines()
            .filter(|line| !line.trim_start().starts_with("debug_logging"))
            .collect::<Vec<_>>()
            .join("\n");
        let parsed: AppConfig =
            toml::from_str(&stripped).expect("old config without debug_logging parses");
        assert!(!parsed.debug_logging);
    }
}
