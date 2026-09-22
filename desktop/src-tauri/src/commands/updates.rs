use super::views::{snapshot, StatusView};
use super::Backend;
use deadass_companion::updater::{self, app_dir, GitHubClient, UpdateCheck};
use tauri::State;

#[tauri::command]
pub async fn check_updates(backend: State<'_, Backend>) -> Result<StatusView, String> {
    let (config, client) = {
        let state = backend.state.lock().await;
        (state.config.clone(), GitHubClient::new())
    };
    let release = client
        .latest_release(&config.updates.repo)
        .await
        .map_err(|error| error.to_string())?;
    let mut state = backend.state.lock().await;
    state.updates.checked = true;
    state.updates.latest_version = updater::version_tuple(&release.tag_name)
        .map(|(major, minor, patch)| format!("{major}.{minor}.{patch}"));
    match updater::compare(updater::CURRENT_VERSION, &release.tag_name) {
        Some(UpdateCheck::Available { version }) => {
            state.updates.available = Some(version.clone());
        }
        Some(UpdateCheck::UpToDate { .. }) => {
            state.updates.available = None;
        }
        None => {}
    }
    drop(state);
    Ok(snapshot(&backend).await)
}

#[tauri::command]
pub async fn update_offsets_now(backend: State<'_, Backend>) -> Result<StatusView, String> {
    let (config, client) = {
        let state = backend.state.lock().await;
        (state.config.clone(), GitHubClient::new())
    };
    let release = client
        .latest_release(&config.updates.repo)
        .await
        .map_err(|error| error.to_string())?;
    let Some(asset) = updater::asset_by_name(&release, "deadass-offsets.toml") else {
        return Err("release has no deadass-offsets.toml asset".into());
    };
    let dir = app_dir();
    let download = dir.join("deadass-offsets.toml.update");
    client
        .download(asset, &download)
        .await
        .map_err(|error| error.to_string())?;
    let installed = updater::install_offsets(&download, config.dll_path.as_deref(), &dir)
        .map_err(|error| error.to_string())?;
    let line = format!(
        "offsets updated: {}",
        installed
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    );
    backend.state.lock().await.push_log(line.clone());
    Ok(snapshot(&backend).await)
}

#[tauri::command]
pub async fn update_app_now(backend: State<'_, Backend>) -> Result<StatusView, String> {
    let (config, client) = {
        let state = backend.state.lock().await;
        (state.config.clone(), GitHubClient::new())
    };
    let release = client
        .latest_release(&config.updates.repo)
        .await
        .map_err(|error| error.to_string())?;
    let dir = app_dir();
    for name in updater::BINARY_ASSETS {
        let Some(asset) = updater::asset_by_name(&release, name) else {
            continue;
        };
        let download = dir.join(format!("{name}.update"));
        client
            .download(asset, &download)
            .await
            .map_err(|error| format!("download {name}: {error}"))?;
    }
    let line = format!(
        "app update v{} staged — restart deadass to apply",
        release.tag_name.trim_start_matches('v')
    );
    backend.state.lock().await.push_log(line);
    Ok(snapshot(&backend).await)
}
