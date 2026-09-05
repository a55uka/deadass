use deadasss_companion::servers::{DllTcpServer, ModHttpServer};
use deadasss_companion::transport::EventBus;
use deadasss_companion::ui::AppState;
use deadasss_companion::{
    ConfigStore, HapticGate, LogTail, ToyHub, default_config_path, discover_console_log,
    resolve_haptic,
};
use std::sync::Arc;
use tokio::sync::Mutex;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let store = ConfigStore::load(default_config_path());
    let state = AppState::new(store.get().clone());
    let config = state.config.clone();
    let (bus, ingress, outlet) = EventBus::new(128);

    tracing::info!(
        dll_port = config.dll_tcp_port,
        mod_port = config.mod_http_port,
        toys = config.buttplug_ws_url,
        "companion starting"
    );

    tokio::spawn(ingress.run(state.input_mode()));
    tokio::spawn(DllTcpServer::new(config.dll_tcp_port, bus.sender()).run());
    tokio::spawn(ModHttpServer::new(config.mod_http_port, bus.sender()).run());
    spawn_log_tail(bus.sender());

    let hub = Arc::new(Mutex::new(ToyHub::new(config.buttplug_ws_url.clone())));
    connect_toys(hub.clone()).await;
    tokio::spawn(run_haptics(outlet, config, hub.clone()));

    tokio::signal::ctrl_c().await?;
    hub.lock().await.disconnect().await;
    Ok(())
}

fn spawn_log_tail(sender: tokio::sync::mpsc::UnboundedSender<deadass_shared::GameEvent>) {
    let Some(location) = discover_console_log() else {
        tracing::warn!("deadlock console.log not found; start steam at least once");
        return;
    };
    if !location.already_created {
        tracing::warn!(
            path = %location.path.display(),
            "console.log not created yet; add -condebug to deadlock launch options, then launch the game"
        );
    }
    tracing::info!(path = %location.path.display(), "tailing deadlock console.log");
    tokio::spawn(LogTail::new(location.path, sender).run());
}

async fn connect_toys(hub: Arc<Mutex<ToyHub>>) {
    let mut locked = hub.lock().await;
    if !central_only() && locked.connect_embedded().await.is_ok() {
        tracing::info!(devices = locked.devices().len(), "embedded toy engine ready");
        return;
    }
    match locked.connect_central().await {
        Ok(()) => tracing::info!(devices = locked.devices().len(), "intiface central ready"),
        Err(error) => tracing::warn!(%error, "no toy backend available"),
    }
}

fn central_only() -> bool {
    std::env::var("DEADASS_TOYS")
        .map(|preference| preference.eq_ignore_ascii_case("central"))
        .unwrap_or(false)
}

async fn run_haptics(
    mut outlet: deadasss_companion::transport::EventOutlet,
    config: deadass_shared::AppConfig,
    hub: Arc<Mutex<ToyHub>>,
) {
    let mut gate = HapticGate::new();
    while let Some(event) = outlet.next().await {
        tracing::debug!(?event, "event accepted");
        let Some(command) = resolve_haptic(&config, &mut gate, event) else {
            continue;
        };
        hub.lock().await.play(command).await;
    }
}
