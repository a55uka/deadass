use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

use super::shock::ShockConfig;
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
    pub(crate) fn preset(strength: f64, duration_ms: u64) -> Self {
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

pub fn resolve_vibrate_targets(selected: Option<&[String]>, connected: &[String]) -> Vec<String> {
    match selected {
        None => connected.to_vec(),
        Some(picks) => connected
            .iter()
            .filter(|name| picks.iter().any(|pick| pick.trim() == *name))
            .cloned()
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppConfig;

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
    fn vibrate_selection_resolves_against_connected_toys() {
        let connected = vec![String::from("Loonie"), String::from("Edge")];
        assert_eq!(resolve_vibrate_targets(None, &connected), connected);
        assert_eq!(
            resolve_vibrate_targets(Some(&[String::from("Edge")]), &connected),
            ["Edge"]
        );
        assert!(resolve_vibrate_targets(Some(&[String::from("Ghost")]), &connected).is_empty());
        assert!(resolve_vibrate_targets(Some(&[]), &connected).is_empty());
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
