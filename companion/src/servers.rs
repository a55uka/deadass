use crate::pipeline::SourceGate;
use crate::ui::AppState;
use axum::{Json, Router, routing::post};
use deadass_shared::{DataSource, EventKind, GameEvent};
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::{Mutex, mpsc};

pub struct ModEventServer {
    port: u16,
    sender: mpsc::UnboundedSender<GameEvent>,
    gate: SourceGate,
    state: Arc<Mutex<AppState>>,
}

impl ModEventServer {
    pub fn new(
        port: u16,
        sender: mpsc::UnboundedSender<GameEvent>,
        gate: SourceGate,
        state: Arc<Mutex<AppState>>,
    ) -> Self {
        Self {
            port,
            sender,
            gate,
            state,
        }
    }

    pub async fn serve(self) -> anyhow::Result<()> {
        let sender = self.sender.clone();
        let gate = self.gate.clone();
        let state = self.state.clone();
        let app = Router::new().route(
            "/event",
            post(move |Json(event): Json<GameEvent>| {
                let sender = sender.clone();
                let gate = gate.clone();
                let state = state.clone();
                async move {
                    let kill_feed = matches!(event.kind, EventKind::Kill | EventKind::Assist);
                    if gate.allows(DataSource::Mod) {
                        state.lock().await.sources.mark_mod_seen();
                        let _ = sender.send(event);
                    } else if kill_feed {
                        let _ = sender.send(event);
                    }
                    "ok"
                }
            }),
        );
        let listener = TcpListener::bind(format!("127.0.0.1:{}", self.port)).await?;
        axum::serve(listener, app).await?;
        Ok(())
    }
}
