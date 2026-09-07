use deadass_companion::pipeline::{self, disconnect, load_config};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_logging();
    let running = pipeline::start(load_config());
    tokio::signal::ctrl_c().await?;
    disconnect(&running.hub).await;
    Ok(())
}

fn init_logging() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
}
