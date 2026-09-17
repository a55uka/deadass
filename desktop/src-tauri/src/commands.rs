use deadass_companion::haptics::HapticCommand;
use deadass_companion::openshock::{ControlCommand, OpenShockClient};
use deadass_companion::pipeline::{self, Pipeline};
use deadass_companion::toys::ToyDevice;
use deadass_shared::{
    filter_shocker_ids, resolve_vibrate_targets, AppConfig, DataSource, Pattern, ShockKind,
    Shocker, TriggerConfig, TriggerKind,
};
use std::sync::Arc;
use tauri::State;

pub type Backend = Arc<Pipeline>;

#[derive(serde::Serialize, Clone)]
pub struct TriggerView {
    kind: String,
    enabled: bool,
    strength: f64,
    duration_ms: u64,
    retrigger_cooldown_ms: u64,
    pattern: String,
    vibrate_devices: Option<Vec<String>>,
    shock_enabled: bool,
    shock_intensity: u8,
    shock_duration_ms: u64,
    shock_kind: String,
    shock_shocker_ids: Option<Vec<String>>,
}

fn trigger_views(config: &AppConfig) -> Vec<TriggerView> {
    let order = [
        TriggerKind::Kill,
        TriggerKind::Death,
        TriggerKind::Assist,
        TriggerKind::Respawn,
        TriggerKind::Parry,
        TriggerKind::Parried,
        TriggerKind::PunchLanded,
        TriggerKind::PunchTaken,
    ];
    let view = |kind: &TriggerKind, rule: &TriggerConfig| TriggerView {
        kind: kind.to_string(),
        enabled: rule.enabled,
        strength: rule.strength,
        duration_ms: rule.duration_ms,
        retrigger_cooldown_ms: rule.retrigger_cooldown_ms,
        pattern: format!("{:?}", rule.pattern).to_lowercase(),
        vibrate_devices: rule.vibrate_devices.clone(),
        shock_enabled: rule.shock.as_ref().is_some_and(|s| s.enabled),
        shock_intensity: rule.shock.as_ref().map(|s| s.intensity).unwrap_or(20),
        shock_duration_ms: rule.shock.as_ref().map(|s| s.duration_ms).unwrap_or(300),
        shock_kind: rule
            .shock
            .as_ref()
            .map(|s| format!("{:?}", s.kind).to_lowercase())
            .unwrap_or_else(|| String::from("shock")),
        shock_shocker_ids: rule.shock.as_ref().and_then(|s| s.shocker_ids.clone()),
    };
    let mut views: Vec<TriggerView> = order
        .into_iter()
        .filter_map(|kind| config.triggers.get(&kind).map(|rule| view(&kind, rule)))
        .collect();
    let mut slot_rules: Vec<(&TriggerKind, &TriggerConfig)> = config
        .triggers
        .iter()
        .filter(|(kind, _)| {
            matches!(
                kind,
                TriggerKind::AbilityUsed { .. } | TriggerKind::AbilityReady { .. }
            )
        })
        .collect();
    slot_rules.sort_by_key(|(kind, _)| match kind {
        TriggerKind::AbilityUsed { slot } => (0, *slot),
        TriggerKind::AbilityReady { slot } => (1, *slot),
        _ => (2, 0),
    });
    for (kind, rule) in slot_rules {
        views.push(view(kind, rule));
    }
    views
}

#[derive(serde::Serialize, Clone)]
pub struct ShockerView {
    id: String,
    name: String,
}

#[derive(serde::Deserialize)]
pub struct ShockerArgs {
    id: String,
    name: String,
}

fn sanitize_roster(entries: Vec<ShockerArgs>) -> Vec<Shocker> {
    let mut shockers: Vec<Shocker> = Vec::new();
    for entry in entries {
        let id = entry.id.trim().to_string();
        if id.is_empty() || shockers.iter().any(|shocker| shocker.id == id) {
            continue;
        }
        if shockers.len() >= 16 {
            break;
        }
        let name = entry.name.trim();
        shockers.push(Shocker {
            id,
            name: if name.is_empty() {
                String::from("Shocker")
            } else {
                name.to_string()
            },
        });
    }
    shockers
}

fn parse_shock_kind(raw: &str) -> ShockKind {
    match raw {
        "vibrate" => ShockKind::Vibrate,
        "sound" => ShockKind::Sound,
        _ => ShockKind::Shock,
    }
}

#[derive(serde::Serialize, Clone)]
pub struct ConfigView {
    master_gain: f64,
    max_strength_cap: f64,
    mute_while_dead: bool,
    debug_logging: bool,
    mod_http_port: u16,
    dll_event_port: u16,
    buttplug_ws_url: String,
    dll_path: Option<String>,
    openshock_enabled: bool,
    openshock_api_token: String,
    openshock_base_url: String,
    openshock_shockers: Vec<ShockerView>,
    openshock_ready: bool,
}

impl ConfigView {
    fn from_config(config: &AppConfig) -> Self {
        Self {
            master_gain: config.master_gain,
            max_strength_cap: config.max_strength_cap,
            mute_while_dead: config.mute_while_dead,
            debug_logging: config.debug_logging,
            mod_http_port: config.mod_http_port,
            dll_event_port: config.dll_event_port,
            buttplug_ws_url: config.buttplug_ws_url.clone(),
            dll_path: config.dll_path.clone(),
            openshock_enabled: config.openshock.enabled,
            openshock_api_token: config.openshock.api_token.clone(),
            openshock_base_url: config.openshock.base_url.clone(),
            openshock_shockers: config
                .openshock
                .shockers
                .iter()
                .map(|shocker| ShockerView {
                    id: shocker.id.clone(),
                    name: shocker.name.clone(),
                })
                .collect(),
            openshock_ready: OpenShockClient::configured(&config.openshock),
        }
    }
}

#[derive(serde::Serialize, Clone)]
pub struct StatusView {
    data_source: String,
    mod_active: bool,
    dll_active: bool,
    inject_phase: String,
    inject_detail: Option<String>,
    log_tailing: bool,
    log_phase: String,
    log_path: Option<String>,
    toy_mode: String,
    devices: Vec<String>,
    toy_error: Option<String>,
    triggers: Vec<TriggerView>,
    log: String,
    log_lines: Vec<String>,
    config: ConfigView,
    config_rev: u64,
    config_path: String,
}

async fn snapshot(backend: &Backend) -> StatusView {
    let hub = backend.hub.lock().await;
    let devices = hub.devices();
    let mode = format!("{:?}", hub.mode());
    drop(hub);
    let state = backend.state.lock().await;
    StatusView {
        data_source: backend.source.get().as_str().to_string(),
        mod_active: state.sources.is_mod_live(),
        dll_active: state.sources.is_dll_live(),
        inject_phase: state.inject.phase.to_string(),
        inject_detail: state.inject.detail.clone(),
        log_tailing: state.is_tailing(),
        log_phase: state.log_phase.to_string(),
        log_path: state.log_path.clone(),
        toy_mode: mode,
        devices: ToyDevice::names(&devices),
        toy_error: state.toys.error.clone(),
        triggers: trigger_views(&state.config),
        log: state.last_log.clone(),
        log_lines: state.log_lines.iter().cloned().collect(),
        config: ConfigView::from_config(&state.config),
        config_rev: state.config_rev,
        config_path: backend.config_path.display().to_string(),
    }
}

#[tauri::command]
pub async fn set_data_source(backend: State<'_, Backend>, source: String) -> Result<StatusView, String> {
    let Some(source) = DataSource::parse(&source) else {
        return Err(format!("unknown data source {source}"));
    };
    pipeline::set_source(&backend, source).await;
    Ok(snapshot(&backend).await)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn set_trigger(
    backend: State<'_, Backend>,
    kind: String,
    enabled: bool,
    strength: f64,
    duration_ms: u64,
    retrigger_cooldown_ms: u64,
    pattern: String,
    vibrate_devices: Option<Vec<String>>,
    shock_enabled: Option<bool>,
    shock_intensity: Option<u8>,
    shock_duration_ms: Option<u64>,
    shock_kind: Option<String>,
    shock_shocker_ids: Option<Vec<String>>,
) -> Result<StatusView, String> {
    let trigger: TriggerKind = kind.parse().map_err(|error: String| error)?;
    let pattern = match pattern.as_str() {
        "pulse" => Pattern::Pulse,
        "ramp" => Pattern::Ramp,
        _ => Pattern::Vibrate,
    };
    pipeline::edit_config(&backend, |config: &mut AppConfig| {
        let rule = config.triggers.entry(trigger).or_default();
        rule.enabled = enabled;
        rule.strength = strength.clamp(0.0, 1.0);
        rule.duration_ms = duration_ms;
        rule.retrigger_cooldown_ms = retrigger_cooldown_ms;
        rule.pattern = pattern;
        if let Some(devices) = vibrate_devices {
            rule.vibrate_devices = Some({
                let mut names: Vec<String> = Vec::new();
                for name in devices {
                    let name = name.trim().to_string();
                    if !name.is_empty() && !names.contains(&name) {
                        names.push(name);
                    }
                }
                names
            });
        }
        rule.shock = match (shock_enabled, shock_intensity, shock_duration_ms) {
            (Some(enabled), Some(intensity), Some(duration_ms)) => {
                let mut next = rule.shock.clone().unwrap_or_default();
                next.enabled = enabled;
                next.intensity = intensity.clamp(1, 100);
                next.duration_ms = duration_ms.clamp(300, 30_000);
                if let Some(kind) = shock_kind.as_deref() {
                    next.kind = parse_shock_kind(kind);
                }
                next.shocker_ids = match shock_shocker_ids.as_deref() {
                    // Untouched selection: every roster shocker.
                    None => None,
                    Some(ids) => Some(filter_shocker_ids(ids, &config.openshock.shockers)),
                };
                Some(next)
            }
            _ => rule.shock.take(),
        };
    })
    .await;
    backend
        .state
        .lock()
        .await
        .push_log(format!("trigger {kind} updated"));
    Ok(snapshot(&backend).await)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn set_config(
    backend: State<'_, Backend>,
    master_gain: Option<f64>,
    max_strength_cap: Option<f64>,
    mute_while_dead: Option<bool>,
    debug_logging: Option<bool>,
    buttplug_ws_url: Option<String>,
    dll_path: Option<String>,
    openshock_enabled: Option<bool>,
    openshock_api_token: Option<String>,
    openshock_base_url: Option<String>,
    openshock_shockers: Option<Vec<ShockerArgs>>,
) -> Result<StatusView, String> {
    let mut central_url: Option<String> = None;
    pipeline::edit_config(&backend, |config: &mut AppConfig| {
        if let Some(value) = master_gain {
            config.master_gain = value.clamp(0.0, 2.0);
        }
        if let Some(value) = max_strength_cap {
            config.max_strength_cap = value.clamp(0.0, 1.0);
        }
        if let Some(value) = mute_while_dead {
            config.mute_while_dead = value;
        }
        if let Some(value) = debug_logging {
            config.debug_logging = value;
        }
        if let Some(url) = buttplug_ws_url.as_deref() {
            let url = url.trim();
            if !url.is_empty() && url != config.buttplug_ws_url {
                config.buttplug_ws_url = url.to_string();
                central_url = Some(url.to_string());
            }
        }
        if let Some(enabled) = openshock_enabled {
            config.openshock.enabled = enabled;
        }
        if let Some(token) = openshock_api_token {
            config.openshock.api_token = token.trim().to_string();
        }
        if let Some(url) = openshock_base_url.as_deref() {
            let url = url.trim().trim_end_matches('/');
            if !url.is_empty() {
                config.openshock.base_url = url.to_string();
            }
        }
        if let Some(entries) = openshock_shockers {
            config.openshock.shockers = sanitize_roster(entries);
            for rule in config.triggers.values_mut() {
                if let Some(shock) = &mut rule.shock {
                    shock.shocker_ids = shock
                        .shocker_ids
                        .as_deref()
                        .map(|ids| filter_shocker_ids(ids, &config.openshock.shockers));
                }
            }
        }
        if let Some(path) = dll_path {
            let path = path.trim();
            config.dll_path = (!path.is_empty()).then(|| path.to_string());
        }
    })
    .await;
    if let Some(url) = central_url {
        backend.hub.lock().await.set_central_url(url);
    }
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
pub async fn test_openshock(backend: State<'_, Backend>) -> Result<StatusView, String> {
    let (config, client) = {
        let state = backend.state.lock().await;
        (state.config.clone(), backend.openshock.clone())
    };
    if !OpenShockClient::configured(&config.openshock) {
        backend
            .state
            .lock()
            .await
            .push_log(String::from(
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
        kind: deadass_shared::ShockKind::Shock,
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
            backend
                .state
                .lock()
                .await
                .push_log(format!(
                    "test {kind}: shock trigger hit but openshock is not configured"
                ));
        }
        None => {}
    }
    Ok(snapshot(&backend).await)
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

fn parse_trigger(kind: &str) -> Result<TriggerKind, String> {
    match kind {
        "punch" => Ok(TriggerKind::PunchLanded),
        "ability" => Ok(TriggerKind::AbilityUsed { slot: 0 }),
        "ready" => Ok(TriggerKind::AbilityReady { slot: 0 }),
        other => other.parse(),
    }
}
