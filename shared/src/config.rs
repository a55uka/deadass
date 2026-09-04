use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::event::EventKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputMode {
    Auto,
    ModOnly,
    MemoryOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Pattern {
    Vibrate,
    Pulse,
    Ramp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TriggerKind {
    Kill,
    Death,
    Assist,
    AbilityUsed { slot: u8 },
    AbilityReady { slot: u8 },
    Respawn,
}

impl TriggerKind {
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
    pub input_mode: InputMode,
    pub dll_tcp_port: u16,
    pub mod_http_port: u16,
    pub buttplug_ws_url: String,
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
            input_mode: InputMode::Auto,
            dll_tcp_port: 24680,
            mod_http_port: 24681,
            buttplug_ws_url: "ws://127.0.0.1:12345".to_string(),
            triggers,
        }
    }
}
