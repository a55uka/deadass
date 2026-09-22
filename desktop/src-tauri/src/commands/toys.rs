use super::views::{snapshot, StatusView};
use super::Backend;
use deadass_companion::haptics::HapticCommand;
use deadass_companion::openshock::{ControlCommand, OpenShockClient};
use deadass_companion::toys::ToyDevice;
use deadass_shared::{resolve_vibrate_targets, Pattern, ShockKind, TriggerKind};
use std::sync::Arc;
use tauri::State;

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
pub async fn test_toys(backend: State<'_, Backend>) -> Result<StatusView, String> {
    let devices = backend.hub.lock().await.devices();
    if devices.is_empty() {
        backend
            .state
            .lock()
            .await
            .push_log(String::from("toys test: no toys connected"));
        return Ok(snapshot(&backend).await);
    }
    let command = HapticCommand {
        strength: 0.5,
        duration_ms: 500,
        pattern: Pattern::Vibrate,
    };
    let names = ToyDevice::names(&devices);
    backend.hub.lock().await.play(command, &names).await;
    backend.state.lock().await.push_log(format!(
        "toys test: vibrate 50% 500ms on {} toy{}",
        devices.len(),
        if devices.len() == 1 { "" } else { "s" },
    ));
    Ok(snapshot(&backend).await)
}

#[tauri::command]
pub async fn test_fire(backend: State<'_, Backend>, kind: String) -> Result<StatusView, String> {
    let trigger = parse_trigger(&kind)?;
    let config = backend.state.lock().await.config.clone();
    let Some(command) = HapticCommand::from_trigger(&config, trigger) else {
        backend
            .state
            .lock()
            .await
            .push_log(format!("test {kind}: disabled in config"));
        return Ok(snapshot(&backend).await);
    };
    let rule = config.trigger(trigger);
    let vibrate_targets = {
        let hub = backend.hub.lock().await;
        let connected = ToyDevice::names(&hub.devices());
        resolve_vibrate_targets(rule.vibrate_devices.as_deref(), &connected)
    };
    let vibrate_target_names = vibrate_targets.join(", ");
    let shock_details = rule.shock.as_ref().and_then(|shock_cfg| {
        ControlCommand::from_shock(shock_cfg)
            .map(|shock| (shock, shock_cfg.target_ids(&config.openshock.shockers)))
    });
    let shock_configured = OpenShockClient::configured(&config.openshock);
    let shock_info = shock_details
        .as_ref()
        .map(|(shock, selected)| (shock.intensity, shock.duration_ms, selected.len()));

    let hub = Arc::clone(&backend.hub);
    let vibrate_targets_len = vibrate_targets.len();
    let vibrate = tauri::async_runtime::spawn(async move {
        hub.lock().await.play(command, &vibrate_targets).await;
    });
    let openshock = Arc::clone(&backend.openshock);
    let openshock_config = config.openshock.clone();
    let shock_result = async move {
        match shock_details {
            Some((shock, selected)) if shock_configured => {
                let fired = openshock
                    .control(&openshock_config, shock, &selected)
                    .await
                    .map(|_| selected.len());
                Some(fired)
            }
            _ => None,
        }
    }
    .await;
    let _ = vibrate.await;

    if vibrate_targets_len == 0 {
        backend.state.lock().await.push_log(format!(
            "test {kind}: vibration skipped, no target toys selected or connected"
        ));
    } else {
        backend.state.lock().await.push_log(format!(
            "test {kind}: vibrate strength={:.2} duration_ms={} pattern={:?} toys={vibrate_targets_len} [{}]",
            command.strength,
            command.duration_ms,
            command.pattern,
            vibrate_target_names,
        ));
    }
    match shock_result {
        Some(Ok(shocker_count)) => {
            let (intensity, duration_ms, _) = shock_info.expect("shock ran, so details exist");
            backend.state.lock().await.push_log(format!(
                "test {kind}: shock intensity={intensity} duration_ms={duration_ms} shockers={shocker_count}",
            ));
        }
        Some(Err(error)) => {
            backend
                .state
                .lock()
                .await
                .push_log(format!("test {kind}: shock failed: {error}"));
        }
        None if shock_info.is_some() => {
            backend.state.lock().await.push_log(format!(
                "test {kind}: shock trigger hit but openshock is not configured"
            ));
        }
        None => {}
    }
    Ok(snapshot(&backend).await)
}

#[tauri::command]
pub async fn test_openshock(backend: State<'_, Backend>) -> Result<StatusView, String> {
    let (config, client) = {
        let state = backend.state.lock().await;
        (state.config.clone(), backend.openshock.clone())
    };
    if !OpenShockClient::configured(&config.openshock) {
        backend.state.lock().await.push_log(String::from(
            "openshock test: not configured (enable it, set a token, add a shocker)",
        ));
        return Ok(snapshot(&backend).await);
    }
    let shocker_ids: Vec<String> = config
        .openshock
        .shockers
        .iter()
        .map(|shocker| shocker.id.trim().to_string())
        .filter(|id| !id.is_empty())
        .collect();
    let intensity = config
        .triggers
        .get(&TriggerKind::Kill)
        .and_then(|rule| rule.shock.as_ref())
        .map(|s| s.intensity)
        .unwrap_or(20)
        .clamp(1, 100);
    let command = ControlCommand {
        kind: ShockKind::Shock,
        intensity,
        duration_ms: 300,
    };
    let outcome = client
        .control(&config.openshock, command, &shocker_ids)
        .await;
    let line = match &outcome {
        Ok(()) => format!(
            "openshock test fired on {} shocker(s): intensity={intensity} duration=300ms",
            shocker_ids.len(),
        ),
        Err(error) => format!("openshock test failed: {error}"),
    };
    backend.state.lock().await.push_log(line);
    Ok(snapshot(&backend).await)
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

fn parse_trigger(kind: &str) -> Result<TriggerKind, String> {
    match kind {
        "punch" => Ok(TriggerKind::PunchLanded),
        "ability" => Ok(TriggerKind::AbilityUsed { slot: 0 }),
        "ready" => Ok(TriggerKind::AbilityReady { slot: 0 }),
        other => other.parse(),
    }
}
