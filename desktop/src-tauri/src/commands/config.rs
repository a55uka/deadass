use super::views::{sanitize_roster, snapshot, ShockerArgs, StatusView};
use super::Backend;
use deadass_companion::pipeline;
use deadass_shared::{filter_shocker_ids, AppConfig, DataSource, Pattern, ShockKind, TriggerKind};
use tauri::State;

fn parse_shock_kind(raw: &str) -> ShockKind {
    match raw {
        "vibrate" => ShockKind::Vibrate,
        "sound" => ShockKind::Sound,
        _ => ShockKind::Shock,
    }
}

#[tauri::command]
pub async fn set_data_source(
    backend: State<'_, Backend>,
    source: String,
) -> Result<StatusView, String> {
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
