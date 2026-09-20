//! Self-updating via GitHub releases.
//!
//! Release convention for `a55uka/deadass`: one release per version, tag
//! `v<major>.<minor>.<patch>` matching the workspace version, with flat
//! assets:
//!
//!   deadass-desktop.exe      the Tauri UI (also drives the pipeline)
//!   deadass-companion.exe    headless pipeline host
//!   deadass_dll.dll          the injectable reader
//!   deadass-offsets.toml     current schema offsets
//!
//! Flow:
//!   * `latest_release`  — GitHub API query + version compare (safe, cheap).
//!   * `update_offsets`  — download the toml asset next to the DLL; the DLL
//!                         re-reads offsets on its next injection, so no
//!                         restart of anything is required beyond that.
//!   * `stage_app`       — download exe/dll assets as `*.update` files next
//!                         to the running app. Swapping happens on the next
//!                         start via [`apply_staged`], because Windows locks
//!                         running executables and DLLs loaded by the game.
//!   * `apply_staged`    — called at pipeline startup before anything locks
//!                         files: move `*.update` over their targets, keep
//!                         the superseded binaries as `*.old` for one cycle.

use serde::Deserialize;
use std::path::{Path, PathBuf};

pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const DEFAULT_REPO: &str = "a55uka/deadass";

const GITHUB_API: &str = "https://api.github.com";
const HTTP_TIMEOUT: Duration = Duration::from_secs(30);
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(300);

use std::time::Duration;

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

    pub async fn download(
        &self,
        asset: &ReleaseAsset,
        destination: &Path,
    ) -> anyhow::Result<()> {
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
    let mut destinations = vec![app_dir.join("deadass-offsets.toml")];
    let dll_dir = dll_directory(config_dll_path, app_dir);
    if dll_dir != app_dir {
        destinations.push(dll_dir.join("deadass-offsets.toml"));
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
        assert_eq!(applied, vec!["deadass_dll".to_string()]);
        assert_eq!(std::fs::read(dir.join("deadass_dll.dll")).unwrap(), b"new");
        assert_eq!(std::fs::read(dir.join("deadass_dll.dll.old")).unwrap(), b"old");
        std::fs::remove_dir_all(&dir).ok();
    }
}
