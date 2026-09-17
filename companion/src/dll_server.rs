use crate::pipeline::SourceGate;
use crate::ui::AppState;
use deadass_shared::{DataSource, GameEvent};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::TcpListener;
use tokio::sync::{Mutex, mpsc};

pub struct DllEventServer {
    port: u16,
    sender: mpsc::UnboundedSender<GameEvent>,
    gate: SourceGate,
    state: Arc<Mutex<AppState>>,
}

impl DllEventServer {
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
        let listener = TcpListener::bind(("127.0.0.1", self.port)).await?;
        loop {
            let (stream, peer) = listener.accept().await?;
            if !peer.ip().is_loopback() {
                continue;
            }
            let task = ConnectionTask {
                sender: self.sender.clone(),
                gate: self.gate.clone(),
                state: self.state.clone(),
            };
            tokio::spawn(task.run(stream));
        }
    }
}

struct ConnectionTask {
    sender: mpsc::UnboundedSender<GameEvent>,
    gate: SourceGate,
    state: Arc<Mutex<AppState>>,
}

impl ConnectionTask {
    async fn run(self, stream: tokio::net::TcpStream) {
        let mut lines = BufReader::new(stream).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let Ok(event) = serde_json::from_str::<GameEvent>(&line) else {
                continue;
            };
            if !self.gate.allows(DataSource::Dll) {
                continue;
            }
            self.state.lock().await.sources.mark_dll_seen();
            let _ = self.sender.send(event);
        }
    }
}
