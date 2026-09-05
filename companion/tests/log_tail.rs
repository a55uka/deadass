use deadass_shared::{EventKind, EventSource};
use deadasss_companion::bridge::LogTail;
use std::io::Write;
use std::time::Duration;

#[tokio::test]
async fn tail_delivers_mod_lines_as_game_events() {
    let path = std::env::temp_dir().join(format!("deadass-tail-{}.log", std::process::id()));
    std::fs::write(&path, "old line\n").unwrap();

    let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
    let worker = tokio::spawn(LogTail::new(path.clone(), sender).run());
    tokio::time::sleep(Duration::from_millis(50)).await;

    let mut log = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
    writeln!(
        log,
        "[DEADASS]{{\"schema\":1,\"event\":\"kill\",\"mod_version\":\"0.1.0\",\"session_id\":\"s\",\"sequence\":3,\"client_time_ms\":99}}"
    )
    .unwrap();
    writeln!(log, "unrelated engine noise").unwrap();

    let event = tokio::time::timeout(Duration::from_secs(2), receiver.recv())
        .await
        .expect("event arrives")
        .expect("channel open");
    assert_eq!(event.source, EventSource::Mod);
    assert_eq!(event.kind, EventKind::Kill);
    assert_eq!(event.sequence, 3);

    worker.abort();
    std::fs::remove_file(&path).ok();
}
