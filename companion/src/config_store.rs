use deadass_shared::AppConfig;
use std::path::{Path, PathBuf};

pub struct ConfigStore {
    path: PathBuf,
    current: AppConfig,
}

impl ConfigStore {
    pub fn load(path: PathBuf) -> Self {
        if let Some(current) = read_config(&path) {
            return Self { path, current };
        }
        let store = Self {
            path,
            current: AppConfig::default(),
        };
        if !store.path.exists()
            && let Err(error) = store.persist()
        {
            tracing::warn!(path = %store.path.display(), %error, "could not create default config");
        }
        store
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
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("deadass")
        .join("config.toml")
}

fn read_config(path: &Path) -> Option<AppConfig> {
    let raw = std::fs::read_to_string(path).ok()?;
    toml::from_str(&raw).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_path(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "deadass-config-{}-{}-{}.toml",
            tag,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ))
    }

    #[test]
    fn load_creates_missing_file_with_defaults() {
        let path = unique_path("missing");
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
        let path = unique_path("invalid");
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
