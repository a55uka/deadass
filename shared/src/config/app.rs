use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::{
    DEFAULT_DLL_EVENT_PORT, DataSource, OpenShockConfig, TriggerConfig, TriggerKind,
};

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
    pub updates: UpdateConfig,
    #[serde(default)]
    pub data_source: DataSource,
    #[serde(default = "default_dll_event_port")]
    pub dll_event_port: u16,
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
            self.openshock.shockers.push(super::Shocker {
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
            updates: UpdateConfig::default(),
            data_source: DataSource::default(),
            dll_event_port: DEFAULT_DLL_EVENT_PORT,
            dll_path: None,
            triggers: Self::default_triggers(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UpdateConfig {
    pub enabled: bool,
    pub repo: String,
    pub auto_update_offsets: bool,
}

impl Default for UpdateConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            repo: String::from("a55uka/deadass"),
            auto_update_offsets: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ShockConfig, Shocker, TriggerConfig};

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
        assert_eq!(
            kill_shock.target_ids(&config.openshock.shockers),
            ["abc123"]
        );
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
        let mut rendered =
            toml::to_string_pretty(&AppConfig::default()).expect("default serializes");
        rendered.push_str("dev_menu = true\n");
        let parsed: AppConfig = toml::from_str(&rendered).expect("unknown keys are ignored");
        assert_eq!(parsed.master_gain, AppConfig::default().master_gain);
    }
}
