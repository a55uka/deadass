use deadass_shared::TriggerKind;
use deadasss_companion::haptics::HapticCommand;
use deadasss_companion::pipeline::Pipeline;
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
    let (mode, devices) = {
        let hub = backend.hub.lock().await;
        (hub.mode(), hub.devices())
    };
    let state = backend.state.lock().await;
    StatusView {
        mod_active: state.sources.mod_active(),
        log_tailing: state.log_tailing(),
        log_phase: state.log_phase.as_str().to_owned(),
        log_path: state.log_path.clone(),
        toy_mode: format!("{mode:?}"),
        devices: devices.iter().map(|device| device.name.clone()).collect(),
        toy_error: state.toys.error.clone(),
        log: state.last_log.clone(),
        log_lines: state.log_lines.iter().cloned().collect(),
    }
}

async fn refresh_toys(backend: &Backend) -> Vec<String> {
    let (mode, devices) = {
        let hub = backend.hub.lock().await;
        (hub.mode(), hub.devices())
    };
    let names: Vec<String> = devices.iter().map(|device| device.name.clone()).collect();
    backend.state.lock().await.note_toys(mode, &devices);
    names
}

fn describe_devices(names: &[String]) -> String {
    if names.is_empty() {
        String::from("no devices yet")
    } else {
        names.join(", ")
    }
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
            let devices = refresh_toys(&backend).await;
            backend
                .state
                .lock()
                .await
                .log(format!("connected embedded: {}", describe_devices(&devices)));
        }
        Err(error) => {
            let line = format!("embedded connect failed: {error}");
            backend.state.lock().await.log(line.clone());
            fail_toys(&backend, error.to_string()).await;
        }
    }
    Ok(snapshot(&backend).await)
}

#[tauri::command]
pub async fn connect_central(backend: State<'_, Backend>) -> Result<StatusView, String> {
    let outcome = backend.hub.lock().await.connect_central().await;
    match outcome {
        Ok(()) => {
            let devices = refresh_toys(&backend).await;
            backend
                .state
                .lock()
                .await
                .log(format!("connected central: {}", describe_devices(&devices)));
        }
        Err(error) => {
            let line = format!("central connect failed: {error}");
            backend.state.lock().await.log(line.clone());
            fail_toys(&backend, error.to_string()).await;
        }
    }
    Ok(snapshot(&backend).await)
}

#[tauri::command]
pub async fn disconnect(backend: State<'_, Backend>) -> Result<StatusView, String> {
    backend.hub.lock().await.disconnect().await;
    refresh_toys(&backend).await;
    backend.state.lock().await.log("disconnected");
    Ok(snapshot(&backend).await)
}

#[tauri::command]
pub async fn rescan(backend: State<'_, Backend>) -> Result<StatusView, String> {
    let outcome = backend.hub.lock().await.rescan().await;
    match outcome {
        Ok(()) => {
            let devices = refresh_toys(&backend).await;
            backend
                .state
                .lock()
                .await
                .log(format!("rescan: {}", describe_devices(&devices)));
        }
        Err(error) => {
            let line = format!("rescan failed: {error}");
            backend.state.lock().await.log(line.clone());
            fail_toys(&backend, error.to_string()).await;
        }
    }
    Ok(snapshot(&backend).await)
}

#[tauri::command]
pub async fn test_fire(backend: State<'_, Backend>, kind: String) -> Result<String, String> {
    let trigger = parse_trigger(&kind)?;
    let config = backend.state.lock().await.config.clone();
    let rule = config.trigger(trigger);
    if !rule.enabled {
        let line = format!("test {kind}: disabled in config");
        backend.state.lock().await.log(line.clone());
        return Ok(line);
    }
    let command = HapticCommand {
        strength: rule.scaled_strength(config.master_gain, config.max_strength_cap),
        duration_ms: rule.duration_ms,
        pattern: rule.pattern,
    };
    backend.hub.lock().await.play(command).await;
    let line = format!(
        "test {kind}: vibrate strength={:.2} duration_ms={} pattern={:?}",
        command.strength, command.duration_ms, command.pattern,
    );
    backend.state.lock().await.log(line.clone());
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
