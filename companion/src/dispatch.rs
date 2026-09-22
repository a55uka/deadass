use crate::haptics::{GateDecision, HapticCommand, HapticGate, SuppressReason};
use crate::openshock::{ControlCommand, OpenShockClient};
use crate::toys::{ToyDevice, ToyHub};
use crate::transport::EventOutlet;
use crate::ui::AppState;
use deadass_shared::{AppConfig, GameEvent, TriggerKind, resolve_vibrate_targets};
use std::sync::Arc;
use tokio::sync::Mutex;

pub async fn run(
    mut outlet: EventOutlet,
    openshock: Arc<OpenShockClient>,
    hub: Arc<Mutex<ToyHub>>,
    state: Arc<Mutex<AppState>>,
) {
    let mut gate = HapticGate::new();
    while let Some(event) = outlet.next().await {
        tracing::debug!(?event, "event accepted");
        let (config, debug_line) = {
            let state = state.lock().await;
            let config = state.config.clone();
            let debug_line = config.debug_logging.then(|| {
                format!(
                    "event {} seq={} wall_ms={}",
                    TriggerKind::from(event.kind),
                    event.sequence,
                    event.wall_time_ms,
                )
            });
            (config, debug_line)
        };
        if let Some(line) = debug_line {
            state.lock().await.push_log(line);
        }
        match gate.decide(&config, event) {
            GateDecision::Fire(command) => {
                fire(&state, &hub, &openshock, &config, event, command).await
            }
            GateDecision::Suppress(reason) if config.debug_logging => {
                report_suppressed(&state, event, reason).await;
            }
            GateDecision::Suppress(_) => {}
        }
    }
}

async fn fire(
    state: &Arc<Mutex<AppState>>,
    hub: &Arc<Mutex<ToyHub>>,
    openshock: &Arc<OpenShockClient>,
    config: &AppConfig,
    event: GameEvent,
    command: HapticCommand,
) {
    let trigger = TriggerKind::from(event.kind);
    let rule = config.trigger(trigger);
    let vibrate_targets = {
        let connected = ToyDevice::names(&hub.lock().await.devices());
        resolve_vibrate_targets(rule.vibrate_devices.as_deref(), &connected)
    };
    if vibrate_targets.is_empty() {
        state.lock().await.push_log(format!(
            "vibrate {trigger}: skipped, no target toys selected or connected"
        ));
    } else {
        state.lock().await.push_log(format!(
            "vibrate {} strength={:.2} duration_ms={} pattern={:?} toys={}",
            trigger,
            command.strength,
            command.duration_ms,
            command.pattern,
            vibrate_targets.len(),
        ));
        let vibrate_hub = Arc::clone(hub);
        tokio::spawn(async move {
            vibrate_hub
                .lock()
                .await
                .play(command, &vibrate_targets)
                .await;
        });
    }

    let shock = rule.shock.as_ref().and_then(ControlCommand::from_shock);
    if let Some(shock) = shock {
        fire_shock(state, openshock, config, trigger, &rule, shock).await;
    }
}

async fn fire_shock(
    state: &Arc<Mutex<AppState>>,
    openshock: &Arc<OpenShockClient>,
    config: &AppConfig,
    trigger: TriggerKind,
    rule: &deadass_shared::TriggerConfig,
    shock: ControlCommand,
) {
    if !OpenShockClient::configured(&config.openshock) {
        state.lock().await.push_log(String::from(
            "openshock trigger hit but openshock is not configured",
        ));
        return;
    }
    let selected = rule
        .shock
        .as_ref()
        .map(|shock| shock.target_ids(&config.openshock.shockers))
        .unwrap_or_default();
    let shocker_count = selected.len();
    let openshock = Arc::clone(openshock);
    let openshock_config = config.openshock.clone();
    tokio::spawn(async move {
        if let Err(error) = openshock.control(&openshock_config, shock, &selected).await {
            tracing::warn!(%error, "openshock control failed");
        }
    });
    state.lock().await.push_log(format!(
        "shock {} intensity={} duration_ms={} kind={} shockers={}",
        trigger,
        shock.intensity,
        shock.duration_ms,
        shock.kind_str(),
        shocker_count,
    ));
}

async fn report_suppressed(state: &Arc<Mutex<AppState>>, event: GameEvent, reason: SuppressReason) {
    let trigger = TriggerKind::from(event.kind);
    state
        .lock()
        .await
        .push_log(format!("suppress {trigger} ({reason})"));
}
