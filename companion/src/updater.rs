use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

use crate::ui::AppState;

pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const DEFAULT_REPO: &str = "a55uka/deadass";

const GITHUB_API: &str = "https://api.github.com";
const HTTP_TIMEOUT: Duration = Duration::from_secs(30);
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(300);
const CHECK_INTERVAL: Duration = Duration::from_secs(30 * 60);
pub const BINARY_ASSETS: &[&str] = &[
    "deadass-desktop.exe",
    "deadass_dll.dll",
    "deadass-companion.exe",
];
const OFFSETS_ASSET: &str = "deadass-offsets.toml";

#[derive(Debug, Clone, Deserialize)]
pub struct Release {
    pub tag_name: String,
    pub name: Option<String>,
    pub assets: Vec<ReleaseAsset>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReleaseAsset {
    pub name: String,
    pub browser_download_url: String,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateCheck {
    Available { version: String },
    UpToDate { version: String },
}

pub fn version_tuple(tag: &str) -> Option<(u64, u64, u64)> {
    let raw = tag.trim().trim_start_matches('v');
    let mut parts = raw.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next().unwrap_or("0").parse().ok()?;
    Some((major, minor, patch))
}

pub fn compare(current: &str, tag: &str) -> Option<UpdateCheck> {
    let current = version_tuple(current)?;
    let latest = version_tuple(tag)?;
    Some(if latest > current {
        UpdateCheck::Available {
            version: tag.trim().trim_start_matches('v').to_string(),
        }
    } else {
        UpdateCheck::UpToDate {
            version: tag.trim().trim_start_matches('v').to_string(),
        }
    })
}

pub fn asset_by_name<'a>(release: &'a Release, name: &str) -> Option<&'a ReleaseAsset> {
    release.assets.iter().find(|asset| asset.name == name)
}

pub struct GitHubClient {
    http: reqwest::Client,
}

impl GitHubClient {
    pub fn new() -> Self {
        let http = reqwest::Client::builder()
            .timeout(HTTP_TIMEOUT)
            .user_agent("deadass-updater")
            .build()
            .expect("reqwest client builds");
        Self { http }
    }

    pub async fn latest_release(&self, repo: &str) -> anyhow::Result<Release> {
        let url = format!("{GITHUB_API}/repos/{repo}/releases/latest");
        let response = self
            .http
            .get(&url)
            .header("Accept", "application/vnd.github+json")
            .send()
            .await?;
        if !response.status().is_success() {
            anyhow::bail!("github returned {}", response.status());
        }
        Ok(response.json::<Release>().await?)
    }

    pub async fn download(&self, asset: &ReleaseAsset, destination: &Path) -> anyhow::Result<()> {
        let response = self
            .http
            .get(&asset.browser_download_url)
            .timeout(DOWNLOAD_TIMEOUT)
            .send()
            .await?;
        if !response.status().is_success() {
            anyhow::bail!("download of {} returned {}", asset.name, response.status());
        }
        let bytes = response.bytes().await?;
        if bytes.len() as u64 != asset.size {
            anyhow::bail!(
                "downloaded {} but expected {} bytes for {}",
                bytes.len(),
                asset.size,
                asset.name
            );
        }
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(destination, &bytes)?;
        Ok(())
    }
}

impl Default for GitHubClient {
    fn default() -> Self {
        Self::new()
    }
}

pub fn app_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.to_path_buf()))
        .unwrap_or_default()
}

pub fn stage(download_path: &Path, target: &Path) -> anyhow::Result<PathBuf> {
    let staged = target.with_extension(format!(
        "{}.update",
        target
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("bin")
    ));
    std::fs::rename(download_path, &staged)?;
    Ok(staged)
}

pub fn apply_staged(app_dir: &Path) -> Vec<String> {
    let mut applied = Vec::new();
    let Ok(entries) = std::fs::read_dir(app_dir) else {
        return applied;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(extension) = path.extension().and_then(|e| e.to_str()) else {
            continue;
        };
        if extension != "update" {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let target = app_dir.join(stem);
        let backup = app_dir.join(format!("{stem}.old"));

        if target.exists() {
            let _ = std::fs::rename(&target, &backup);
        }
        match std::fs::rename(&path, &target) {
            Ok(()) => applied.push(stem.to_string()),
            Err(error) => {
                tracing::warn!(%error, file = %stem, "could not apply staged update");
                let _ = std::fs::rename(&backup, &target);
            }
        }
    }
    applied
}

pub fn dll_directory(config_dll_path: Option<&str>, app_dir: &Path) -> PathBuf {
    config_dll_path
        .and_then(|raw| {
            let path = PathBuf::from(raw);
            path.parent().map(|parent| parent.to_path_buf())
        })
        .unwrap_or_else(|| app_dir.to_path_buf())
}

pub fn install_offsets(
    download_path: &Path,
    config_dll_path: Option<&str>,
    app_dir: &Path,
) -> anyhow::Result<Vec<PathBuf>> {
    let mut installed = Vec::new();
    let mut destinations = vec![app_dir.join(OFFSETS_ASSET)];
    let dll_dir = dll_directory(config_dll_path, app_dir);
    if dll_dir != app_dir {
        destinations.push(dll_dir.join(OFFSETS_ASSET));
    }
    let contents = std::fs::read(download_path)?;
    for destination in destinations {
        std::fs::create_dir_all(
            destination
                .parent()
                .map(|parent| parent.to_path_buf())
                .unwrap_or_default(),
        )?;
        std::fs::write(&destination, &contents)?;
        installed.push(destination);
    }
    std::fs::remove_file(download_path).ok();
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
            let line = format!("update check failed: {error}");
            tracing::warn!("{line}");
            state.lock().await.push_log(line);
        }
        tokio::time::sleep(CHECK_INTERVAL).await;
    }
}

async fn run_check_cycle(
    state: &Arc<Mutex<AppState>>,
    config: &deadass_shared::AppConfig,
) -> anyhow::Result<()> {
    let http = GitHubClient::new();
    let release = http.latest_release(&config.updates.repo).await?;
    let dir = app_dir();

    {
        let mut status = state.lock().await;
        status.updates.checked = true;
        status.updates.latest_version = version_tuple(&release.tag_name)
            .map(|(major, minor, patch)| format!("{major}.{minor}.{patch}"));
    }

    let Some(UpdateCheck::Available { version }) = compare(CURRENT_VERSION, &release.tag_name)
    else {
        state.lock().await.updates.available = None;
        return Ok(());
    };

    {
        let mut status = state.lock().await;
        status.updates.available = Some(version.clone());
        status.push_log(format!(
            "update available: v{version} (running v{}) — downloading in background",
            CURRENT_VERSION
        ));
    }

    if config.updates.auto_update_offsets {
        update_offsets(&http, state, config, &release, &dir).await;
    }
    stage_binaries(&http, state, &release, &dir).await;
    Ok(())
}

async fn update_offsets(
    http: &GitHubClient,
    state: &Arc<Mutex<AppState>>,
    config: &deadass_shared::AppConfig,
    release: &Release,
    dir: &Path,
) {
    let Some(asset) = asset_by_name(release, OFFSETS_ASSET) else {
        return;
    };
    let download = dir.join(format!("{OFFSETS_ASSET}.update"));
    let outcome = match http.download(asset, &download).await {
        Ok(()) => match install_offsets(&download, config.dll_path.as_deref(), dir) {
            Ok(paths) => Ok(format!(
                "offsets updated: {}",
                paths
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
            Err(error) => Err(format!("offsets update failed: {error}")),
        },
        Err(error) => Err(format!("offsets download failed: {error}")),
    };
    report(state, outcome).await;
}

async fn stage_binaries(
    http: &GitHubClient,
    state: &Arc<Mutex<AppState>>,
    release: &Release,
    dir: &Path,
) {
    for name in BINARY_ASSETS {
        let Some(asset) = asset_by_name(release, name) else {
            continue;
        };
        let staged = dir.join(format!("{name}.update"));
        if staged.exists() {
            continue;
        }
        let outcome = match http.download(asset, &staged).await {
            Ok(()) => Ok(format!("staged {name} for next restart")),
            Err(error) => Err(format!("staging {name} failed: {error}")),
        };
        report(state, outcome).await;
    }
}

async fn report(state: &Arc<Mutex<AppState>>, outcome: Result<String, String>) {
    match outcome {
        Ok(line) => {
            tracing::info!("{line}");
            state.lock().await.push_log(line);
        }
        Err(line) => {
            tracing::warn!("{line}");
            state.lock().await.push_log(line);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_tags_parse_and_compare() {
        assert_eq!(version_tuple("v0.2.0"), Some((0, 2, 0)));
        assert_eq!(version_tuple("0.2"), Some((0, 2, 0)));
        assert_eq!(version_tuple("release-1"), None);
        assert_eq!(
            compare("0.1.0", "v0.2.0"),
            Some(UpdateCheck::Available {
                version: "0.2.0".into()
            })
        );
        assert_eq!(
            compare("0.2.0", "v0.2.0"),
            Some(UpdateCheck::UpToDate {
                version: "0.2.0".into()
            })
        );
        assert_eq!(compare("nonsense", "v0.2.0"), None);
    }

    #[test]
    fn staging_uses_update_extension() {
        let dir = std::env::temp_dir().join(format!("deadass-stage-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let download = dir.join("downloaded.bin");
        std::fs::write(&download, b"new").unwrap();
        let staged = stage(&download, &dir.join("deadass-desktop.exe")).unwrap();
        assert_eq!(staged, dir.join("deadass-desktop.exe.update"));
        assert!(!download.exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn apply_staged_swaps_and_backs_up() {
        let dir = std::env::temp_dir().join(format!("deadass-apply-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("deadass_dll.dll"), b"old").unwrap();
        std::fs::write(dir.join("deadass_dll.dll.update"), b"new").unwrap();

        let applied = apply_staged(&dir);
        assert_eq!(applied, vec!["deadass_dll.dll".to_string()]);
        assert_eq!(std::fs::read(dir.join("deadass_dll.dll")).unwrap(), b"new");
        assert_eq!(
            std::fs::read(dir.join("deadass_dll.dll.old")).unwrap(),
            b"old"
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}
