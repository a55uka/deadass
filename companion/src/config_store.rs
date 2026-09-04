use deadass_shared::AppConfig;
use std::path::{Path, PathBuf};

pub struct ConfigStore {
    path: PathBuf,
    current: AppConfig,
}

impl ConfigStore {
    pub fn load(path: PathBuf) -> Self {
        let current = read_config(&path).unwrap_or_default();
        Self { path, current }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn get(&self) -> &AppConfig {
        &self.current
    }

    pub fn update(&mut self, next: AppConfig) {
        self.current = next;
    }

    pub fn persist(&self) -> anyhow::Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let rendered = toml::to_string_pretty(&self.current)?;
        std::fs::write(&self.path, rendered)?;
        Ok(())
    }
}

pub fn default_config_path() -> PathBuf {
    std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("~/.config").expand_home())
        .join("deadass")
        .join("config.toml")
}

fn read_config(path: &Path) -> Option<AppConfig> {
    let raw = std::fs::read_to_string(path).ok()?;
    toml::from_str(&raw).ok()
}

trait ExpandHome {
    fn expand_home(self) -> PathBuf;
}

impl ExpandHome for PathBuf {
    fn expand_home(self) -> PathBuf {
        let rendered = self.to_string_lossy().to_string();
        if let Some(stripped) = rendered.strip_prefix("~/")
            && let Ok(home) = std::env::var("HOME")
        {
            return PathBuf::from(home).join(stripped);
        }
        self
    }
}
