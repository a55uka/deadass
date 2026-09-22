use crate::config_store::{ConfigStore, default_config_path};
use crate::config_watch;
use crate::dispatch;
use crate::dll_server::DllEventServer;
use crate::game_log::discover_console_log;
use crate::injector;
use crate::log_tail::LogTail;
use crate::openshock::OpenShockClient;
use crate::servers::ModEventServer;
use crate::toys::{ConnectionMode, ToyDevice, ToyHub};
use crate::transport::EventBus;
use crate::ui::AppState;
use crate::updater;
use deadass_shared::{AppConfig, DataSource, GameEvent};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use tokio::sync::{Mutex, mpsc};

pub struct Pipeline {
    pub hub: Arc<Mutex<ToyHub>>,
    pub state: Arc<Mutex<AppState>>,
    pub source: SourceGate,
    pub config_path: PathBuf,
    pub openshock: Arc<OpenShockClient>,
}

#[derive(Debug, Clone, Default)]
pub struct SourceGate(Arc<AtomicU8>);

impl SourceGate {
    pub fn new(source: DataSource) -> Self {
        Self(Arc::new(AtomicU8::new(gate_value(source))))
    }

    pub fn get(&self) -> DataSource {
        match self.0.load(Ordering::Relaxed) {
            1 => DataSource::Dll,
            _ => DataSource::Mod,
        }
    }

    pub fn set(&self, source: DataSource) {
        self.0.store(gate_value(source), Ordering::Relaxed);
    }

    pub fn allows(&self, source: DataSource) -> bool {
        self.get() == source
    }
}

fn gate_value(source: DataSource) -> u8 {
    match source {
        DataSource::Mod => 0,
        DataSource::Dll => 1,
    }
}

pub fn load_config() -> AppConfig {
    ConfigStore::load(default_config_path()).get().clone()
}

pub fn start(config: AppConfig) -> Pipeline {
    let (bus, ingress, outlet) = EventBus::new(128);
    let hub = Arc::new(Mutex::new(ToyHub::new(config.buttplug_ws_url.clone())));
    let state = Arc::new(Mutex::new(AppState::new(config.clone())));
    let gate = SourceGate::new(config.data_source);

    tracing::info!(
        mod_port = config.mod_http_port,
        dll_port = config.dll_event_port,
        source = config.data_source.as_str(),
        toys = config.buttplug_ws_url,
        "pipeline starting"
    );

    tokio::spawn(ingress.run());
    tokio::spawn(
        ModEventServer::new(
            config.mod_http_port,
            bus.sender(),
            gate.clone(),
            state.clone(),
        )
        .serve(),
    );
    spawn_log_tail(bus.sender(), gate.clone(), state.clone());
    tokio::spawn(
        DllEventServer::new(
            config.dll_event_port,
            bus.sender(),
            gate.clone(),
            state.clone(),
        )
        .serve(),
    );
    injector::spawn_supervisor(
        gate.clone(),
        state.clone(),
        injector::resolve_dll_path(&config),
    );
    apply_staged_updates(&state);
    spawn_toy_autoconnect(hub.clone(), state.clone());
    let openshock = Arc::new(OpenShockClient::new());
    tokio::spawn(dispatch::run(
        outlet,
        openshock.clone(),
        hub.clone(),
        state.clone(),
    ));
    updater::spawn_checks(state.clone());
    config_watch::spawn(default_config_path(), state.clone());

    Pipeline {
        hub,
        state,
        source: gate,
        config_path: default_config_path(),
        openshock,
    }
}

fn apply_staged_updates(state: &Arc<Mutex<AppState>>) {
    let applied = updater::apply_staged(&updater::app_dir());
    if applied.is_empty() {
        return;
    }
    let line = format!("applied staged updates: {}", applied.join(", "));
    tracing::info!("{line}");
    if let Ok(mut locked) = state.try_lock() {
        locked.push_log(line);
    }
}

pub async fn edit_config(pipeline: &Pipeline, edit: impl FnOnce(&mut AppConfig)) -> AppConfig {
    let mut config = pipeline.state.lock().await.config.clone();
    edit(&mut config);
    pipeline.state.lock().await.replace_config(config.clone());
    let mut store = ConfigStore::load(pipeline.config_path.clone());
    store.set(config.clone());
    if let Err(error) = store.persist() {
        tracing::warn!(%error, "could not persist config edit");
    }
    config
}

pub async fn set_source(pipeline: &Pipeline, source: DataSource) {
    pipeline.source.set(source);
    edit_config(pipeline, |config| config.data_source = source).await;
    let mut state = pipeline.state.lock().await;
    state.push_log(format!("game data source set to {}", source.as_str()));
}

pub async fn disconnect(hub: &Arc<Mutex<ToyHub>>) {
    hub.lock().await.disconnect().await;
}

fn spawn_log_tail(
    sender: mpsc::UnboundedSender<GameEvent>,
    gate: SourceGate,
    state: Arc<Mutex<AppState>>,
) {
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
    tokio::spawn(LogTail::with_state(location.path, sender, gate, state).run());
}

fn waiting_hint(path: &str) -> String {
    format!(
        "waiting for console.log to be created; add -condebug to deadlock launch options: {path}"
    )
}

#[derive(Debug, Clone, Copy)]
enum ToyAutoconnect {
    EmbeddedThenCentral,
    CentralOnly,
}

impl ToyAutoconnect {
    fn from_env() -> Option<Self> {
        match std::env::var("DEADASS_TOYS").as_deref() {
            Ok(choice) if choice.eq_ignore_ascii_case("embedded") => {
                Some(Self::EmbeddedThenCentral)
            }
            Ok(choice) if choice.eq_ignore_ascii_case("central") => Some(Self::CentralOnly),
            _ => None,
        }
    }

    async fn connect(preference: Self, hub: Arc<Mutex<ToyHub>>, state: Arc<Mutex<AppState>>) {
        let mut toys = hub.lock().await;
        let outcome = match preference {
            Self::EmbeddedThenCentral if toys.connect_embedded().await.is_ok() => {
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
}

fn spawn_toy_autoconnect(hub: Arc<Mutex<ToyHub>>, state: Arc<Mutex<AppState>>) {
    if let Some(preference) = ToyAutoconnect::from_env() {
        tokio::spawn(ToyAutoconnect::connect(preference, hub, state));
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
