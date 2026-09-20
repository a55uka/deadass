use crate::config_store::{ConfigStore, default_config_path};
use crate::config_watch;
use crate::dll_server::DllEventServer;
use crate::injector;
use crate::log_tail::LogTail;
use crate::openshock::{ControlCommand, OpenShockClient};
use crate::updater::{self, GitHubClient, UpdateCheck};
use crate::servers::ModEventServer;
use crate::toys::{ConnectionMode, ToyDevice, ToyHub};
use crate::transport::{EventBus, EventOutlet};
use crate::ui::AppState;
use crate::{HapticGate, discover_console_log};
use crate::{GateDecision, SuppressReason};
use deadass_shared::{
    resolve_vibrate_targets, AppConfig, DataSource, GameEvent, TriggerKind,
};
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
        ModEventServer::new(config.mod_http_port, bus.sender(), gate.clone(), state.clone())
            .serve(),
    );
    spawn_log_tail(bus.sender(), gate.clone(), state.clone());
    tokio::spawn(
        DllEventServer::new(config.dll_event_port, bus.sender(), gate.clone(), state.clone())
            .serve(),
    );
    injector::spawn_supervisor(
        gate.clone(),
        state.clone(),
        injector::resolve_dll_path(&config),
    );
    // Swaps in any staged app updates from a previous session before
    // anything locks the new binaries.
    let applied = updater::apply_staged(&std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.to_path_buf()))
        .unwrap_or_default());
    if !applied.is_empty() {
        let line = format!("applied staged updates: {}", applied.join(", "));
        tracing::info!("{line}");
        if let Ok(mut state) = state.try_lock() {
            state.push_log(line);
        }
    }

    spawn_toy_autoconnect(hub.clone(), state.clone());
    let openshock = std::sync::Arc::new(OpenShockClient::new());
    tokio::spawn(run_haptics(outlet, openshock.clone(), hub.clone(), state.clone()));
    tokio::spawn(run_update_checks(state.clone()));
    config_watch::spawn(default_config_path(), state.clone());

    Pipeline {
        hub,
        state,
        source: gate,
        config_path: default_config_path(),
        openshock,
    }
}

pub async fn edit_config(
    pipeline: &Pipeline,
    edit: impl FnOnce(&mut AppConfig),
) -> AppConfig {
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

/// Cadence of the background update check.
const UPDATE_CHECK_INTERVAL: std::time::Duration = std::time::Duration::from_secs(30 * 60);

/// Background updater: queries GitHub for a newer release, auto-installs a
/// fresh deadass-offsets.toml, and pre-downloads (stages) new binaries so an
/// app update is applied on the next restart — all without user interaction.
async fn run_update_checks(state: Arc<Mutex<AppState>>) {
    loop {
        let config = state.lock().await.config.clone();
        if config.updates.enabled {
            if let Err(error) = run_update_cycle(&state, &config).await {
                let line = format!("update check failed: {error}");
                tracing::warn!("{line}");
                state.lock().await.push_log(line);
            }
        }
        tokio::time::sleep(UPDATE_CHECK_INTERVAL).await;
    }
}

/// One background check cycle against the configured repository.
async fn run_update_cycle(
    state: &Arc<Mutex<AppState>>,
    config: &AppConfig,
) -> anyhow::Result<()> {
    let http = GitHubClient::new();
    let release = http.latest_release(&config.updates.repo).await?;

    let app_dir = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.to_path_buf()))
        .unwrap_or_default();
    {
        let mut status = state.lock().await;
        status.updates.checked = true;
        status.updates.latest_version = updater::version_tuple(&release.tag_name)
            .map(|(major, minor, patch)| format!("{major}.{minor}.{patch}"));
    }

    let Some(UpdateCheck::Available { version }) =
        updater::compare(updater::CURRENT_VERSION, &release.tag_name)
    else {
        state.lock().await.updates.available = None;
        return Ok(());
    };

    {
        let mut status = state.lock().await;
        status.updates.available = Some(version.clone());
        status.push_log(format!(
            "update available: v{version} (running v{}) — downloading in background",
            updater::CURRENT_VERSION
        ));
    }

    // Auto-install the offsets toml: safe schema data, applied on next game
    // launch.
    if config.updates.auto_update_offsets {
        if let Some(asset) = updater::asset_by_name(&release, "deadass-offsets.toml") {
            let download = app_dir.join("deadass-offsets.toml.update");
            match http.download(asset, &download).await {
                Ok(()) => match updater::install_offsets(
                    &download,
                    config.dll_path.as_deref(),
                    &app_dir,
                ) {
                    Ok(paths) => {
                        let line = format!(
                            "offsets updated: {}",
                            paths
                                .iter()
                                .map(|path| path.display().to_string())
                                .collect::<Vec<_>>()
                                .join(", ")
                        );
                        tracing::info!("{line}");
                        state.lock().await.push_log(line);
                    }
                    Err(error) => {
                        let line = format!("offsets update failed: {error}");
                        tracing::warn!("{line}");
                        state.lock().await.push_log(line);
                    }
                },
                Err(error) => {
                    let line = format!("offsets download failed: {error}");
                    tracing::warn!("{line}");
                    state.lock().await.push_log(line);
                }
            }
        }
    }

    // Stage the new binaries in the background; apply_staged at the next
    // startup swaps them in.
    for name in ["deadass-desktop.exe", "deadass_dll.dll", "deadass-companion.exe"] {
        let Some(asset) = updater::asset_by_name(&release, name) else {
            continue;
        };
        let staged = app_dir.join(format!("{name}.update"));
        if staged.exists() {
            continue;
        }
        match http.download(asset, &staged).await {
            Ok(()) => {
                let line = format!("staged {name} for next restart");
                tracing::info!("{line}");
                state.lock().await.push_log(line);
            }
            Err(error) => {
                let line = format!("staging {name} failed: {error}");
                tracing::warn!("{line}");
                state.lock().await.push_log(line);
            }
        }
    }
    Ok(())
}

async fn run_haptics(
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
                let trigger = TriggerKind::from(event.kind);
                let rule = config.trigger(trigger);
                // An empty vibrate_devices selection means every toy.
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
                    let vibrate_hub = Arc::clone(&hub);
                    tokio::spawn(async move {
                        vibrate_hub.lock().await.play(command, &vibrate_targets).await;
                    });
                }
                
                let shock = rule.shock.as_ref().and_then(ControlCommand::from_shock);
                if let Some(shock) = shock {
                    if OpenShockClient::configured(&config.openshock) {
                        let selected = rule
                            .shock
                            .as_ref()
                            .map(|shock| shock.target_ids(&config.openshock.shockers))
                            .unwrap_or_default();
                        let shocker_count = selected.len();
                        let openshock = Arc::clone(&openshock);
                        let openshock_config = config.openshock.clone();
                        tokio::spawn(async move {
                            if let Err(error) =
                                openshock.control(&openshock_config, shock, &selected).await
                            {
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
                    } else {
                        state
                            .lock()
                            .await
                            .push_log(String::from("openshock trigger hit but openshock is not configured"));
                    }
                }
            }
            GateDecision::Suppress(reason) if config.debug_logging => {
                report_suppressed(&state, event, reason).await;
            }
            GateDecision::Suppress(_) => {}
        }
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
