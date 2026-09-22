use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShockKind {
    #[default]
    Shock,
    Vibrate,
    Sound,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShockConfig {
    pub enabled: bool,
    pub intensity: u8,
    pub duration_ms: u64,
    pub kind: ShockKind,
    #[serde(default)]
    pub shocker_ids: Option<Vec<String>>,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Shocker {
    pub id: String,
    pub name: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::TriggerConfig;

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
        assert_eq!(
            shock.shocker_ids,
            Some(vec![String::from("abc"), String::from("def")])
        );
    }
}
