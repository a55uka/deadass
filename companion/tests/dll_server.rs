use deadass_companion::dll_server::DllEventServer;
use deadass_companion::pipeline::SourceGate;
use deadass_companion::ui::AppState;
use deadass_shared::{AppConfig, DataSource, EventKind, GameEvent};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn spawn_server(gate: SourceGate) -> (u16, tokio::sync::mpsc::UnboundedReceiver<GameEvent>) {
    let port = free_port();
    let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
    let state = Arc::new(tokio::sync::Mutex::new(AppState::new(AppConfig::default())));
    tokio::spawn(DllEventServer::new(port, sender, gate, state).serve());
    (port, receiver)
}

async fn send_lines(port: u16, payload: &str) {
    for _ in 0..50 {
        if let Ok(mut stream) = TcpStream::connect(("127.0.0.1", port)).await {
            stream.write_all(payload.as_bytes()).await.unwrap();
            stream.flush().await.unwrap();
            // Hold the connection open briefly so the server reads the lines.
            tokio::time::sleep(Duration::from_millis(50)).await;
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("dll event server never came up on {port}");
}

#[tokio::test]
async fn dll_events_flow_into_the_bus() {
    let (port, mut receiver) = spawn_server(SourceGate::new(DataSource::Dll));
    let event = GameEvent::new(7, 42, EventKind::AbilityUsed { slot: 3 });
    let payload = format!("{}\n", serde_json::to_string(&event).unwrap());
    send_lines(port, &payload).await;

    let received = tokio::time::timeout(Duration::from_secs(2), receiver.recv())
        .await
        .expect("event arrives")
        .expect("channel open");
    assert_eq!(received.kind, EventKind::AbilityUsed { slot: 3 });
    assert_eq!(received.sequence, 7);
}

#[tokio::test]
async fn dll_events_are_gated_when_mod_source_is_selected() {
    let (port, mut receiver) = spawn_server(SourceGate::new(DataSource::Mod));
    let event = GameEvent::new(9, 43, EventKind::Kill);
    let payload = format!("{}\n", serde_json::to_string(&event).unwrap());
    send_lines(port, &payload).await;

    let outcome = tokio::time::timeout(Duration::from_millis(300), receiver.recv()).await;
    assert!(
        outcome.is_err(),
        "dll events must be swallowed while the mod source is selected"
    );
}

#[tokio::test]
async fn garbage_lines_are_ignored_and_good_lines_still_parse() {
    let (port, mut receiver) = spawn_server(SourceGate::new(DataSource::Dll));
    let event = GameEvent::new(11, 44, EventKind::Death);
    let payload = format!(
        "not json at all\n{}\n[DEADASS] console noise\n",
        serde_json::to_string(&event).unwrap()
    );
    send_lines(port, &payload).await;

    let received = tokio::time::timeout(Duration::from_secs(2), receiver.recv())
        .await
        .expect("valid event arrives")
        .expect("channel open");
    assert_eq!(received.kind, EventKind::Death);
}
