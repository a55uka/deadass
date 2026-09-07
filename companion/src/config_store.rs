use deadass_shared::AppConfig;
use std::path::{Path, PathBuf};

pub struct ConfigStore {
    path: PathBuf,
    current: AppConfig,
}

impl ConfigStore {
    pub fn load(path: PathBuf) -> Self {
        if let Some(current) = parse_file(&path) {
            return Self { path, current };
        }
        let store = Self {
            path,
            current: AppConfig::default(),
        };
        store.create_default_file();
        store
    }

    pub fn get(&self) -> &AppConfig {
        &self.current
    }

    fn create_default_file(&self) {
        if self.path.exists() {
            return;
        }
        if let Err(error) = self.persist() {
            tracing::warn!(path = %self.path.display(), %error, "could not create default config");
        }
    }

    fn persist(&self) -> anyhow::Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&self.path, toml::to_string_pretty(&self.current)?)?;
        Ok(())
    }
}

pub fn default_config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("deadass")
        .join("config.toml")
}

fn parse_file(path: &Path) -> Option<AppConfig> {
    toml::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_config_path(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "deadass-config-{tag}-{}-{}.toml",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|nanos| nanos.as_nanos())
                .unwrap_or(0)
        ))
    }

    #[test]
    fn load_creates_missing_file_with_defaults() {
        let path = temp_config_path("missing");
        assert!(!path.exists());

        let store = ConfigStore::load(path.clone());
        assert_eq!(
            store.get().mod_http_port,
            AppConfig::default().mod_http_port
        );

        let raw = std::fs::read_to_string(&path).expect("config file created");
        let parsed: AppConfig = toml::from_str(&raw).expect("created file parses");
        assert_eq!(parsed.mod_http_port, AppConfig::default().mod_http_port);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn load_does_not_overwrite_invalid_file() {
        let path = temp_config_path("invalid");
        std::fs::write(&path, "not valid toml [[[").unwrap();

        let store = ConfigStore::load(path.clone());
        assert_eq!(
            store.get().mod_http_port,
            AppConfig::default().mod_http_port
        );

        let raw = std::fs::read_to_string(&path).unwrap();
        assert_eq!(raw, "not valid toml [[[");
        std::fs::remove_file(&path).ok();
    }
}
