use crate::transport::parse_newline_events;
use axum::{Json, Router, routing::post};
use deadass_shared::GameEvent;
use tokio::io::AsyncReadExt;
use tokio::net::TcpListener;
use tokio::sync::mpsc;

pub struct ModHttpServer {
    port: u16,
    sender: mpsc::UnboundedSender<GameEvent>,
}

impl ModHttpServer {
    pub fn new(port: u16, sender: mpsc::UnboundedSender<GameEvent>) -> Self {
        Self { port, sender }
    }

    pub async fn run(self) -> anyhow::Result<()> {
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

pub struct DllTcpServer {
    port: u16,
    sender: mpsc::UnboundedSender<GameEvent>,
}

impl DllTcpServer {
    pub fn new(port: u16, sender: mpsc::UnboundedSender<GameEvent>) -> Self {
        Self { port, sender }
    }

    pub async fn run(self) -> anyhow::Result<()> {
        let listener = TcpListener::bind(format!("127.0.0.1:{}", self.port)).await?;
        loop {
            let (mut socket, _) = listener.accept().await?;
            let sender = self.sender.clone();
            tokio::spawn(async move {
                let mut buffer = vec![0u8; 8192];
                while let Ok(read) = socket.read(&mut buffer).await {
                    if read == 0 {
                        break;
                    }
                    for event in parse_newline_events(&buffer[..read]) {
                        let _ = sender.send(event);
                    }
                }
            });
        }
    }
}
