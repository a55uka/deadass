use axum::{Json, Router, routing::post};
use deadass_shared::GameEvent;
use tokio::net::TcpListener;
use tokio::sync::mpsc;

pub struct ModEventServer {
    port: u16,
    sender: mpsc::UnboundedSender<GameEvent>,
}

impl ModEventServer {
    pub fn new(port: u16, sender: mpsc::UnboundedSender<GameEvent>) -> Self {
        Self { port, sender }
    }

    pub async fn serve(self) -> anyhow::Result<()> {
        let sender = self.sender.clone();
        let app = Router::new().route(
            "/event",
            post(move |Json(event): Json<GameEvent>| {
                let sender = sender.clone();
                async move {
                    let _ = sender.send(event);
                    "ok"
                }
            }),
        );
        let listener = TcpListener::bind(format!("127.0.0.1:{}", self.port)).await?;
        axum::serve(listener, app).await?;
        Ok(())
    }
}
