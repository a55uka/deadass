use deadass_companion::haptics::HapticCommand;
use deadass_companion::pipeline::Pipeline;
use deadass_companion::toys::ToyDevice;
use deadass_shared::TriggerKind;
use std::sync::Arc;
use tauri::State;

pub type Backend = Arc<Pipeline>;

#[derive(serde::Serialize, Clone)]
pub struct StatusView {
    mod_active: bool,
    log_tailing: bool,
    log_phase: String,
    log_path: Option<String>,
    toy_mode: String,
    devices: Vec<String>,
    toy_error: Option<String>,
    log: String,
    log_lines: Vec<String>,
}

async fn snapshot(backend: &Backend) -> StatusView {
    let hub = backend.hub.lock().await;
    let devices = hub.devices();
    let mode = format!("{:?}", hub.mode());
    drop(hub);
    let state = backend.state.lock().await;
    StatusView {
        mod_active: state.sources.is_live(),
        log_tailing: state.is_tailing(),
        log_phase: state.log_phase.to_string(),
        log_path: state.log_path.clone(),
        toy_mode: mode,
        devices: ToyDevice::names(&devices),
        toy_error: state.toys.error.clone(),
        log: state.last_log.clone(),
        log_lines: state.log_lines.iter().cloned().collect(),
    }
}

async fn sync_toys(backend: &Backend) -> Vec<String> {
    let hub = backend.hub.lock().await;
    let mode = hub.mode();
    let devices = hub.devices();
    drop(hub);
    let names = ToyDevice::names(&devices);
    backend.state.lock().await.set_toys(mode, &devices);
    names
}

async fn finish_toy_action(
    backend: &Backend,
    outcome: Result<Vec<String>, String>,
    label: &str,
) -> StatusView {
    match outcome {
        Ok(devices) => {
            let line = format!("{label}: {}", describe_devices(&devices));
            backend.state.lock().await.push_log(line);
        }
        Err(error) => {
            let line = format!("{label} failed: {error}");
            let mut state = backend.state.lock().await;
            state.push_log(line);
            state.set_toy_error(error);
        }
    }
    snapshot(backend).await
}

fn describe_devices(names: &[String]) -> String {
    if names.is_empty() {
        String::from("no devices yet")
    } else {
        names.join(", ")
    }
}

#[tauri::command]
pub async fn get_status(backend: State<'_, Backend>) -> Result<StatusView, String> {
    Ok(snapshot(&backend).await)
}

#[tauri::command]
pub async fn connect_embedded(backend: State<'_, Backend>) -> Result<StatusView, String> {
    let outcome = backend.hub.lock().await.connect_embedded().await;
    let outcome = match outcome {
        Ok(()) => Ok(sync_toys(&backend).await),
        Err(error) => Err(error.to_string()),
    };
    Ok(finish_toy_action(&backend, outcome, "connected embedded").await)
}

#[tauri::command]
pub async fn connect_central(backend: State<'_, Backend>) -> Result<StatusView, String> {
    let outcome = backend.hub.lock().await.connect_central().await;
    let outcome = match outcome {
        Ok(()) => Ok(sync_toys(&backend).await),
        Err(error) => Err(error.to_string()),
    };
    Ok(finish_toy_action(&backend, outcome, "connected central").await)
}

#[tauri::command]
pub async fn disconnect(backend: State<'_, Backend>) -> Result<StatusView, String> {
    backend.hub.lock().await.disconnect().await;
    sync_toys(&backend).await;
    backend.state.lock().await.push_log("disconnected");
    Ok(snapshot(&backend).await)
}

#[tauri::command]
pub async fn rescan(backend: State<'_, Backend>) -> Result<StatusView, String> {
    let outcome = backend.hub.lock().await.rescan().await;
    let outcome = match outcome {
        Ok(()) => Ok(sync_toys(&backend).await),
        Err(error) => Err(error.to_string()),
    };
    Ok(finish_toy_action(&backend, outcome, "rescan").await)
}

#[tauri::command]
pub async fn test_fire(backend: State<'_, Backend>, kind: String) -> Result<String, String> {
    let trigger = parse_trigger(&kind)?;
    let config = backend.state.lock().await.config.clone();
    let Some(command) = HapticCommand::from_trigger(&config, trigger) else {
        let line = format!("test {kind}: disabled in config");
        backend.state.lock().await.push_log(line.clone());
        return Ok(line);
    };
    backend.hub.lock().await.play(command).await;
    let line = format!(
        "test {kind}: vibrate strength={:.2} duration_ms={} pattern={:?}",
        command.strength, command.duration_ms, command.pattern,
    );
    backend.state.lock().await.push_log(line.clone());
    Ok(line)
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
