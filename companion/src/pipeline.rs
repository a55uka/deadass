use crate::servers::ModHttpServer;
use crate::toys::{ConnectionMode, ToyDevice, ToyHub};
use crate::transport::{EventBus, EventOutlet};
use crate::ui::AppState;
use crate::{
    ConfigStore, HapticGate, LogTail, default_config_path, discover_console_log, resolve_haptic,
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

    tokio::spawn(ingress.run(config.input_mode));
    tokio::spawn(ModHttpServer::new(config.mod_http_port, bus.sender()).run());
    spawn_log_tail(bus.sender(), state.clone());
    tokio::spawn(connect_toys(hub.clone(), state.clone()));
    tokio::spawn(run_haptics(outlet, config, hub.clone(), state.clone()));

    Pipeline { hub, state }
}

pub async fn disconnect(hub: &Arc<Mutex<ToyHub>>) {
    hub.lock().await.disconnect().await;
}

fn spawn_log_tail(sender: mpsc::UnboundedSender<GameEvent>, state: Arc<Mutex<AppState>>) {
    let Some(location) = discover_console_log() else {
        note(
            state,
            "deadlock console.log not found; start steam at least once",
        );
        return;
    };
    if !location.already_created {
        note(
            state.clone(),
            format!(
                "console.log not created yet; add -condebug to deadlock launch options: {}",
                location.path.display()
            ),
        );
    }
    note(
        state.clone(),
        format!("tailing {}", location.path.display()),
    );
    tokio::spawn(LogTail::new(location.path, sender).run());
}

async fn connect_toys(hub: Arc<Mutex<ToyHub>>, state: Arc<Mutex<AppState>>) {
    enum Outcome {
        Ready(ConnectionMode, Vec<ToyDevice>, &'static str),
        Failed(String),
    }
    let outcome = {
        let mut locked = hub.lock().await;
        if !central_only() && locked.connect_embedded().await.is_ok() {
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
    let mut gate = HapticGate::new();
    while let Some(event) = outlet.next().await {
        tracing::debug!(?event, "event accepted");
        state.lock().await.note_source(event.source);
        let Some(command) = resolve_haptic(&config, &mut gate, event) else {
            continue;
        };
        hub.lock().await.play(command).await;
    }
}

fn note(state: Arc<Mutex<AppState>>, line: impl Into<String>) {
    let line = line.into();
    tracing::warn!("{line}");
    tokio::spawn(async move {
        state.lock().await.log(line);
    });
}

fn central_only() -> bool {
    std::env::var("DEADASS_TOYS")
        .map(|preference| preference.eq_ignore_ascii_case("central"))
        .unwrap_or(false)
}
