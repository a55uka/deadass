use crate::log_tail::LogTail;
use crate::servers::ModEventServer;
use crate::toys::{ConnectionMode, ToyDevice, ToyHub};
use crate::transport::{EventBus, EventOutlet};
use crate::ui::AppState;
use crate::{ConfigStore, HapticGate, default_config_path, discover_console_log};
use crate::{GateDecision, SuppressReason};
use deadass_shared::{AppConfig, GameEvent, TriggerKind};
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};

pub struct Pipeline {
    pub hub: Arc<Mutex<ToyHub>>,
    pub state: Arc<Mutex<AppState>>,
}

pub fn load_config() -> AppConfig {
    ConfigStore::load(default_config_path()).get().clone()
}

pub fn start(config: AppConfig) -> Pipeline {
    let (bus, ingress, outlet) = EventBus::new(128);
    let hub = Arc::new(Mutex::new(ToyHub::new(config.buttplug_ws_url.clone())));
    let state = Arc::new(Mutex::new(AppState::new(config.clone())));

    tracing::info!(
        mod_port = config.mod_http_port,
        toys = config.buttplug_ws_url,
        "pipeline starting"
    );

    tokio::spawn(ingress.run());
    tokio::spawn(ModEventServer::new(config.mod_http_port, bus.sender()).serve());
    spawn_log_tail(bus.sender(), state.clone());
    spawn_toy_autoconnect(hub.clone(), state.clone());
    tokio::spawn(run_haptics(outlet, config, hub.clone(), state.clone()));

    Pipeline { hub, state }
}

pub async fn disconnect(hub: &Arc<Mutex<ToyHub>>) {
    hub.lock().await.disconnect().await;
}

fn spawn_log_tail(sender: mpsc::UnboundedSender<GameEvent>, state: Arc<Mutex<AppState>>) {
    let Some(location) = discover_console_log() else {
        report_missing(
            &state,
            "deadlock console.log not found; start steam at least once",
        );
        return;
    };
    let path = location.path.display().to_string();
    if location.already_created {
        report_tailing(&state, path.clone(), format!("tailing {path}"));
    } else {
        report_waiting(&state, path.clone(), waiting_hint(&path));
    }
    tokio::spawn(LogTail::with_state(location.path, sender, state).run());
}

fn waiting_hint(path: &str) -> String {
    format!(
        "waiting for console.log to be created; add -condebug to deadlock launch options: {path}"
    )
}

fn spawn_toy_autoconnect(hub: Arc<Mutex<ToyHub>>, state: Arc<Mutex<AppState>>) {
    let preference = match std::env::var("DEADASS_TOYS").as_deref() {
        Ok(choice) if choice.eq_ignore_ascii_case("embedded") => {
            ToyAutoconnect::EmbeddedThenCentral
        }
        Ok(choice) if choice.eq_ignore_ascii_case("central") => ToyAutoconnect::CentralOnly,
        _ => return,
    };
    tokio::spawn(connect_toys(hub, state, preference));
}

#[derive(Debug, Clone, Copy)]
enum ToyAutoconnect {
    EmbeddedThenCentral,
    CentralOnly,
}

async fn connect_toys(
    hub: Arc<Mutex<ToyHub>>,
    state: Arc<Mutex<AppState>>,
    preference: ToyAutoconnect,
) {
    let mut toys = hub.lock().await;
    let outcome = match preference {
        ToyAutoconnect::EmbeddedThenCentral if toys.connect_embedded().await.is_ok() => {
            Attached::ready(&toys, "embedded toy engine ready")
        }
        _ => match toys.connect_central().await {
            Ok(()) => Attached::ready(&toys, "intiface central ready"),
            Err(error) => Attached::failed(error.to_string()),
        },
    };
    drop(toys);
    let mut state = state.lock().await;
    match outcome {
        Attached::Ready {
            line,
            mode,
            devices,
        } => {
            tracing::info!("{line}");
            state.set_toys(mode, &devices);
            state.push_log(line);
        }
        Attached::Failed { line } => {
            tracing::warn!("{line}");
            state.push_log(line);
        }
    }
}

enum Attached {
    Ready {
        line: String,
        mode: ConnectionMode,
        devices: Vec<ToyDevice>,
    },
    Failed {
        line: String,
    },
}

impl Attached {
    fn ready(hub: &ToyHub, label: &str) -> Self {
        let devices = hub.devices();
        Self::Ready {
            line: format!("{label} devices={}", devices.len()),
            mode: hub.mode(),
            devices,
        }
    }

    fn failed(error: String) -> Self {
        Self::Failed {
            line: format!("no toy backend available: {error}"),
        }
    }
}

async fn run_haptics(
    mut outlet: EventOutlet,
    config: AppConfig,
    hub: Arc<Mutex<ToyHub>>,
    state: Arc<Mutex<AppState>>,
) {
    let mut gate = HapticGate::new();
    while let Some(event) = outlet.next().await {
        tracing::debug!(?event, "event accepted");
        report_event(&state, &config, event).await;
        match gate.decide(&config, event) {
            GateDecision::Fire(command) => {
                hub.lock().await.play(command).await;
                let trigger = TriggerKind::from(event.kind);
                state.lock().await.push_log(format!(
                    "vibrate {} strength={:.2} duration_ms={} pattern={:?}",
                    trigger, command.strength, command.duration_ms, command.pattern,
                ));
            }
            GateDecision::Suppress(reason) if config.debug_logging => {
                report_suppressed(&state, event, reason).await;
            }
            GateDecision::Suppress(_) => {}
        }
    }
}

async fn report_event(state: &Arc<Mutex<AppState>>, config: &AppConfig, event: GameEvent) {
    let mut state = state.lock().await;
    state.mark_source_seen();
    if config.debug_logging {
        let trigger = TriggerKind::from(event.kind);
        state.push_log(format!(
            "event {} seq={} wall_ms={}",
            trigger, event.sequence, event.wall_time_ms,
        ));
    }
}

async fn report_suppressed(state: &Arc<Mutex<AppState>>, event: GameEvent, reason: SuppressReason) {
    let trigger = TriggerKind::from(event.kind);
    state
        .lock()
        .await
        .push_log(format!("suppress {trigger} ({reason})"));
}

fn report_tailing(state: &Arc<Mutex<AppState>>, path: String, line: String) {
    report_log_status(state, move |state| state.set_tailing(path, line));
}

fn report_waiting(state: &Arc<Mutex<AppState>>, path: String, line: String) {
    report_log_status(state, move |state| state.set_waiting(path, line));
}

fn report_missing(state: &Arc<Mutex<AppState>>, line: impl Into<String>) {
    let line = line.into();
    report_log_status(state, move |state| state.set_missing(line));
}

fn report_log_status(
    state: &Arc<Mutex<AppState>>,
    update: impl FnOnce(&mut AppState) + Send + 'static,
) {
    tracing::warn!("log status update");
    if let Ok(mut locked) = state.try_lock() {
        update(&mut locked);
    } else {
        let state = Arc::clone(state);
        tokio::spawn(async move {
            let mut guarded = state.lock().await;
            update(&mut guarded);
        });
    }
}
