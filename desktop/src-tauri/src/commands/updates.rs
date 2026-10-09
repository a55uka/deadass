use super::views::{snapshot, StatusView};
use super::Backend;
use deadass_companion::updater;
use tauri::State;

/// Fetch deadass-offsets.toml from GitHub main and install it when the
/// content changed. This is the only update the app performs.
#[tauri::command]
pub async fn update_offsets_now(backend: State<'_, Backend>) -> Result<StatusView, String> {
    let config = backend.state.lock().await.config.clone();
    updater::update_offsets_now(&backend.state, &config)
        .await
        .map_err(|error| error.to_string())?;
    Ok(snapshot(&backend).await)
}
