use deadass_shared::TriggerKind;
use deadasss_companion::haptics::HapticCommand;
use deadasss_companion::pipeline::Pipeline;
use std::sync::Arc;
use tauri::State;

pub type Backend = Arc<Pipeline>;

#[derive(serde::Serialize, Clone)]
pub struct StatusView {
    mod_active: bool,
    dll_active: bool,
    external_active: bool,
    toy_mode: String,
    devices: Vec<String>,
    toy_error: Option<String>,
    log: String,
}

async fn snapshot(backend: &Backend) -> StatusView {
    let (mode, devices) = {
        let hub = backend.hub.lock().await;
        (hub.mode(), hub.devices())
    };
    let state = backend.state.lock().await;
    StatusView {
        mod_active: state.sources.mod_active(),
        dll_active: state.sources.dll_active(),
        external_active: state.sources.external_active(),
        toy_mode: format!("{mode:?}"),
        devices: devices.iter().map(|device| device.name.clone()).collect(),
        toy_error: state.toys.error.clone(),
        log: state.last_log.clone(),
    }
}

async fn refresh_toys(backend: &Backend) {
    let (mode, devices) = {
        let hub = backend.hub.lock().await;
        (hub.mode(), hub.devices())
    };
    backend.state.lock().await.note_toys(mode, &devices);
}

async fn fail_toys(backend: &Backend, error: String) {
    backend.state.lock().await.note_toy_error(error);
}

#[tauri::command]
pub async fn get_status(backend: State<'_, Backend>) -> Result<StatusView, String> {
    Ok(snapshot(&backend).await)
}

#[tauri::command]
pub async fn connect_embedded(backend: State<'_, Backend>) -> Result<StatusView, String> {
    let outcome = backend.hub.lock().await.connect_embedded().await;
    match outcome {
        Ok(()) => {
            refresh_toys(&backend).await;
            backend.state.lock().await.log("embedded connect attempted");
        }
        Err(error) => fail_toys(&backend, error.to_string()).await,
    }
    Ok(snapshot(&backend).await)
}

#[tauri::command]
pub async fn connect_central(backend: State<'_, Backend>) -> Result<StatusView, String> {
    let outcome = backend.hub.lock().await.connect_central().await;
    match outcome {
        Ok(()) => {
            refresh_toys(&backend).await;
            backend.state.lock().await.log("central connect attempted");
        }
        Err(error) => fail_toys(&backend, error.to_string()).await,
    }
    Ok(snapshot(&backend).await)
}

#[tauri::command]
pub async fn disconnect(backend: State<'_, Backend>) -> Result<StatusView, String> {
    backend.hub.lock().await.disconnect().await;
    refresh_toys(&backend).await;
    Ok(snapshot(&backend).await)
}

#[tauri::command]
pub async fn rescan(backend: State<'_, Backend>) -> Result<StatusView, String> {
    let outcome = backend.hub.lock().await.rescan().await;
    if let Err(error) = outcome {
        fail_toys(&backend, error.to_string()).await;
    }
    Ok(snapshot(&backend).await)
}

#[tauri::command]
pub async fn test_fire(backend: State<'_, Backend>, kind: String) -> Result<String, String> {
    let trigger = parse_trigger(&kind)?;
    let config = backend.state.lock().await.config.clone();
    let rule = config.trigger(trigger);
    if !rule.enabled {
        return Ok(format!("{kind} disabled in config"));
    }
    backend
        .hub
        .lock()
        .await
        .play(HapticCommand {
            strength: rule.scaled_strength(config.master_gain, config.max_strength_cap),
            duration_ms: rule.duration_ms,
            pattern: rule.pattern,
        })
        .await;
    Ok(format!("fired {kind}"))
}

fn parse_trigger(kind: &str) -> Result<TriggerKind, String> {
    match kind {
        "kill" => Ok(TriggerKind::Kill),
        "death" => Ok(TriggerKind::Death),
        "assist" => Ok(TriggerKind::Assist),
        "respawn" => Ok(TriggerKind::Respawn),
        "ability" => Ok(TriggerKind::AbilityUsed { slot: 0 }),
        "ready" => Ok(TriggerKind::AbilityReady { slot: 0 }),
        other => Err(format!("unknown trigger {other}")),
    }
}
