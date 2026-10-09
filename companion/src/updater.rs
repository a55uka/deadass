//! Offsets syncing. The companion pulls `deadass-offsets.toml` from the
//! project's GitHub main branch and installs it next to the DLL — that is
//! the only thing that ever updates; binaries are never touched.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;

use crate::ui::AppState;

pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const DEFAULT_REPO: &str = "a55uka/deadass";

const OFFSETS_BRANCH: &str = "main";
const OFFSETS_FILE: &str = "deadass-offsets.toml";
const FETCH_TIMEOUT: Duration = Duration::from_secs(30);
const CHECK_INTERVAL: Duration = Duration::from_secs(30 * 60);

pub fn offsets_url(repo: &str) -> String {
    format!("https://raw.githubusercontent.com/{repo}/{OFFSETS_BRANCH}/{OFFSETS_FILE}")
}

pub fn app_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.to_path_buf()))
        .unwrap_or_default()
}

pub fn dll_directory(config_dll_path: Option<&str>, app_dir: &Path) -> PathBuf {
    config_dll_path
        .and_then(|raw| {
            let path = PathBuf::from(raw);
            path.parent().map(|parent| parent.to_path_buf())
        })
        .unwrap_or_else(|| app_dir.to_path_buf())
}

pub struct GitHubClient {
    http: reqwest::Client,
}

impl GitHubClient {
    pub fn new() -> Self {
        let http = reqwest::Client::builder()
            .timeout(FETCH_TIMEOUT)
            .user_agent("deadass-updater")
            .build()
            .expect("reqwest client builds");
        Self { http }
    }

    pub async fn fetch_offsets(&self, repo: &str) -> anyhow::Result<String> {
        let url = offsets_url(repo);
        let response = self.http.get(&url).send().await?;
        if !response.status().is_success() {
            anyhow::bail!("fetch of {url} returned {}", response.status());
        }
        Ok(response.text().await?)
    }
}

impl Default for GitHubClient {
    fn default() -> Self {
        Self::new()
    }
}

/// Write the fetched toml everywhere it belongs (app dir + the DLL's dir
/// when different). Returns only the paths whose content actually changed.
pub fn install_offsets(
    contents: &str,
    config_dll_path: Option<&str>,
    app_dir: &Path,
) -> anyhow::Result<Vec<PathBuf>> {
    let mut installed = Vec::new();
    let mut destinations = vec![app_dir.join(OFFSETS_FILE)];
    let dll_dir = dll_directory(config_dll_path, app_dir);
    if dll_dir != app_dir {
        destinations.push(dll_dir.join(OFFSETS_FILE));
    }
    for destination in destinations {
        if std::fs::read_to_string(&destination).is_ok_and(|existing| existing == contents) {
            continue;
        }
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&destination, contents)?;
        installed.push(destination);
    }
    Ok(installed)
}

pub fn spawn_checks(state: Arc<Mutex<AppState>>) {
    tokio::spawn(run_checks(state));
}

async fn run_checks(state: Arc<Mutex<AppState>>) {
    loop {
        let config = state.lock().await.config.clone();
        if config.updates.enabled
            && let Err(error) = run_check_cycle(&state, &config).await
        {
            let line = format!("offsets sync failed: {error}");
            tracing::warn!("{line}");
            state.lock().await.push_log(line);
        }
        tokio::time::sleep(CHECK_INTERVAL).await;
    }
}

/// Fetch and install the current offsets; quiet when unchanged.
pub async fn update_offsets_now(
    state: &Arc<Mutex<AppState>>,
    config: &deadass_shared::AppConfig,
) -> anyhow::Result<()> {
    let contents = GitHubClient::new()
        .fetch_offsets(&config.updates.repo)
        .await?;
    let dir = app_dir();
    let installed = install_offsets(&contents, config.dll_path.as_deref(), &dir)?;
    let mut locked = state.lock().await;
    locked.updates.checked = true;
    if installed.is_empty() {
        return Ok(());
    }
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_millis() as u64)
        .unwrap_or(0);
    locked.updates.updated = Some(now_ms);
    let line = format!(
        "offsets updated from {}: {}",
        config.updates.repo,
        installed
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    );
    tracing::info!("{line}");
    locked.push_log(line);
    Ok(())
}

async fn run_check_cycle(state: &Arc<Mutex<AppState>>, config: &deadass_shared::AppConfig) -> anyhow::Result<()> {
    update_offsets_now(state, config).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offsets_url_points_at_raw_main() {
        assert_eq!(
            offsets_url("a55uka/deadass"),
            "https://raw.githubusercontent.com/a55uka/deadass/main/deadass-offsets.toml"
        );
    }

    #[test]
    fn install_writes_only_changed_destinations() {
        let dir = std::env::temp_dir().join(format!("deadass-inst-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let dll_dir = dir.join("gamedir");
        std::fs::create_dir_all(&dll_dir).unwrap();

        // first install: both destinations are new
        let installed = install_offsets("v1", dll_dir.join("d.dll").to_str(), &dir)
            .unwrap();
        assert_eq!(installed.len(), 2);

        // identical content: nothing changes
        let installed = install_offsets("v1", dll_dir.join("d.dll").to_str(), &dir)
            .unwrap();
        assert!(installed.is_empty());

        // new content: both change
        let installed = install_offsets("v2", dll_dir.join("d.dll").to_str(), &dir)
            .unwrap();
        assert_eq!(installed.len(), 2);
        assert_eq!(std::fs::read(dir.join(OFFSETS_FILE)).unwrap(), b"v2");
        std::fs::remove_dir_all(&dir).ok();
    }
}
