use super::Backend;
use deadass_companion::openshock::OpenShockClient;
use deadass_companion::toys::ToyDevice;
use deadass_companion::updater;
use deadass_shared::{AppConfig, TriggerConfig, TriggerKind};

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

pub fn sanitize_roster(entries: Vec<ShockerArgs>) -> Vec<deadass_shared::Shocker> {
    let mut shockers: Vec<deadass_shared::Shocker> = Vec::new();
    for entry in entries {
        let id = entry.id.trim().to_string();
        if id.is_empty() || shockers.iter().any(|shocker| shocker.id == id) {
            continue;
        }
        if shockers.len() >= 16 {
            break;
        }
        let name = entry.name.trim();
        shockers.push(deadass_shared::Shocker {
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
    app_version: String,
    update: UpdateStatusView,
}

#[derive(serde::Serialize, Clone)]
pub struct UpdateStatusView {
    checks_enabled: bool,
    latest_version: Option<String>,
    available: Option<String>,
}

pub async fn snapshot(backend: &Backend) -> StatusView {
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
        app_version: updater::CURRENT_VERSION.to_string(),
        update: UpdateStatusView {
            checks_enabled: state.config.updates.enabled,
            latest_version: state.updates.latest_version.clone(),
            available: state.updates.available.clone(),
        },
    }
}
