use crate::servers::ModHttpServer;
use crate::toys::{ConnectionMode, ToyDevice, ToyHub};
use crate::transport::{EventBus, EventOutlet};
use crate::ui::AppState;
use crate::{
    ConfigStore, GateDecision, HapticGate, LogTail, SuppressReason, decide_haptic,
    default_config_path, discover_console_log,
};
use deadass_shared::{AppConfig, GameEvent};
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
    tokio::spawn(ModHttpServer::new(config.mod_http_port, bus.sender()).run());
    spawn_log_tail(bus.sender(), state.clone());
    maybe_autoconnect_toys(hub.clone(), state.clone());
    tokio::spawn(run_haptics(outlet, config, hub.clone(), state.clone()));

    Pipeline { hub, state }
}

pub async fn disconnect(hub: &Arc<Mutex<ToyHub>>) {
    hub.lock().await.disconnect().await;
}

fn spawn_log_tail(sender: mpsc::UnboundedSender<GameEvent>, state: Arc<Mutex<AppState>>) {
    let Some(location) = discover_console_log() else {
        set_missing(
            &state,
            "deadlock console.log not found; start steam at least once",
        );
        return;
    };
    let path_display = location.path.display().to_string();
    if location.already_created {
        set_tailing(
            &state,
            path_display.clone(),
            format!("tailing {path_display}"),
        );
    } else {
        set_waiting(
            &state,
            path_display.clone(),
            format!(
                "waiting for console.log to be created; add -condebug to deadlock launch options: {path_display}"
            ),
        );
    }
    tokio::spawn(LogTail::with_state(location.path, sender, state).run());
}

fn maybe_autoconnect_toys(hub: Arc<Mutex<ToyHub>>, state: Arc<Mutex<AppState>>) {
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
    enum Outcome {
        Ready(ConnectionMode, Vec<ToyDevice>, &'static str),
        Failed(String),
    }
    let outcome = {
        let mut locked = hub.lock().await;
        if matches!(preference, ToyAutoconnect::EmbeddedThenCentral)
            && locked.connect_embedded().await.is_ok()
        {
            Outcome::Ready(locked.mode(), locked.devices(), "embedded toy engine ready")
        } else {
            match locked.connect_central().await {
                Ok(()) => Outcome::Ready(locked.mode(), locked.devices(), "intiface central ready"),
                Err(error) => Outcome::Failed(error.to_string()),
            }
        }
    };
    let mut state = state.lock().await;
    match outcome {
        Outcome::Ready(mode, devices, label) => {
            let line = format!("{label} devices={}", devices.len());
            tracing::info!("{line}");
            state.note_toys(mode, &devices);
            state.log(line);
        }
        Outcome::Failed(error) => {
            let line = format!("no toy backend available: {error}");
            tracing::warn!("{line}");
            state.log(line);
        }
    }
}

async fn run_haptics(
    mut outlet: EventOutlet,
    config: AppConfig,
    hub: Arc<Mutex<ToyHub>>,
    state: Arc<Mutex<AppState>>,
) {
    use deadass_shared::TriggerKind;
    let debug = config.debug_logging;
    let mut gate = HapticGate::new();
    while let Some(event) = outlet.next().await {
        tracing::debug!(?event, "event accepted");
        let trigger = TriggerKind::from_event(event.kind);
        {
            let mut state = state.lock().await;
            state.note_source();
            if debug {
                state.log(format!(
                    "event {} seq={} wall_ms={}",
                    trigger.key(),
                    event.sequence,
                    event.wall_time_ms,
                ));
            }
        }
        match decide_haptic(&config, &mut gate, event) {
            GateDecision::Fire(command) => {
                hub.lock().await.play(command).await;
                state.lock().await.log(format!(
                    "vibrate {} strength={:.2} duration_ms={} pattern={:?}",
                    trigger.key(),
                    command.strength,
                    command.duration_ms,
                    command.pattern,
                ));
            }
            GateDecision::Suppress(reason) => {
                if debug {
                    let why = match reason {
                        SuppressReason::Disabled => "disabled in config",
                        SuppressReason::MutedWhileDead => "muted while dead",
                        SuppressReason::Cooldown => "retrigger cooldown",
                    };
                    state
                        .lock()
                        .await
                        .log(format!("suppress {} ({why})", trigger.key()));
                }
            }
        }
    }
}

fn set_tailing(state: &Arc<Mutex<AppState>>, path: String, line: String) {
    tracing::warn!("{line}");
    if let Ok(mut locked) = state.try_lock() {
        locked.note_log_tail(path, line);
    } else {
        let state = Arc::clone(state);
        tokio::spawn(async move {
            state.lock().await.note_log_tail(path, line);
        });
    }
}

fn set_waiting(state: &Arc<Mutex<AppState>>, path: String, line: String) {
    tracing::warn!("{line}");
    if let Ok(mut locked) = state.try_lock() {
        locked.note_log_waiting(path, line);
    } else {
        let state = Arc::clone(state);
        tokio::spawn(async move {
            state.lock().await.note_log_waiting(path, line);
        });
    }
}

fn set_missing(state: &Arc<Mutex<AppState>>, line: impl Into<String>) {
    let line = line.into();
    tracing::warn!("{line}");
    if let Ok(mut locked) = state.try_lock() {
        locked.note_log_missing(line);
    } else {
        let state = Arc::clone(state);
        tokio::spawn(async move {
            state.lock().await.note_log_missing(line);
        });
    }
}
