use serde::{Deserialize, Serialize};

use super::shock::Shocker;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpenShockConfig {
    pub enabled: bool,
    pub api_token: String,
    pub base_url: String,
    #[serde(default)]
    pub shocker_id: String,
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
