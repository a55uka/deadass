mod app;
mod openshock;
mod shock;
mod trigger;

pub use app::AppConfig;
pub use openshock::OpenShockConfig;
pub use shock::{ShockConfig, ShockKind, Shocker, filter_shocker_ids};
pub use trigger::{Pattern, TriggerConfig, TriggerFamily, TriggerKind, resolve_vibrate_targets};

use serde::{Deserialize, Serialize};

pub const DEFAULT_DLL_EVENT_PORT: u16 = 24680;

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
