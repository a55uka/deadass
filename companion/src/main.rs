use deadasss_companion::servers::{DllTcpServer, ModHttpServer};
use deadasss_companion::transport::EventBus;
use deadasss_companion::ui::AppState;
use deadasss_companion::{ConfigStore, default_config_path};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let store = ConfigStore::load(default_config_path());
    let state = AppState::new(store.get().clone());
    let (bus, ingress, _outlet) = EventBus::new(128);
    let ingress_mode = state.input_mode();

    tracing::info!(
        dll_port = state.config.dll_tcp_port,
        mod_port = state.config.mod_http_port,
        "companion starting"
    );

    tokio::spawn(ingress.run(ingress_mode));
    tokio::spawn(DllTcpServer::new(state.config.dll_tcp_port, bus.sender()).run());
    tokio::spawn(ModHttpServer::new(state.config.mod_http_port, bus.sender()).run());

    tokio::signal::ctrl_c().await?;
    Ok(())
}
