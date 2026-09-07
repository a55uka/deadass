use deadass_companion::log_tail::LogTail;
use deadass_shared::EventKind;
use std::io::Write;
use std::time::Duration;

#[tokio::test]
async fn tail_delivers_mod_lines_as_game_events() {
    let path = std::env::temp_dir().join(format!("deadass-tail-{}.log", std::process::id()));
    std::fs::write(&path, "old line\n").unwrap();

    let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
    let worker = tokio::spawn(LogTail::new(path.clone(), sender).run());
    tokio::time::sleep(Duration::from_millis(50)).await;

    let mut log = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap();
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
    assert_eq!(event.kind, EventKind::Kill);
    assert_eq!(event.sequence, 3);

    worker.abort();
    std::fs::remove_file(&path).ok();
}

#[tokio::test]
async fn tail_waits_for_file_creation_then_delivers() {
    use deadass_companion::ui::AppState;
    use std::sync::Arc;

    let path = std::env::temp_dir().join(format!("deadass-tail-wait-{}.log", std::process::id()));
    std::fs::remove_file(&path).ok();
    assert!(!path.exists());

    let state = Arc::new(tokio::sync::Mutex::new(AppState::new(
        deadass_shared::AppConfig::default(),
    )));
    let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
    let worker = tokio::spawn(LogTail::with_state(path.clone(), sender, state.clone()).run());

    // Give the worker a chance to observe the missing file and report waiting.
    tokio::time::sleep(Duration::from_millis(700)).await;
    {
        let locked = state.lock().await;
        assert_eq!(locked.log_phase.to_string(), "waiting");
        assert!(!locked.is_tailing());
        assert!(locked.last_log.contains("waiting for console.log"));
    }

    // Create the file with stale history, then append a live event.
    std::fs::write(&path, "stale history\n").unwrap();
    tokio::time::sleep(Duration::from_millis(700)).await;
    {
        let locked = state.lock().await;
        assert_eq!(locked.log_phase.to_string(), "tailing");
        assert!(locked.is_tailing());
    }
    // Stale history must not produce events; only new appends do.
    assert!(receiver.try_recv().is_err());

    let mut log = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap();
    writeln!(
        log,
        "[DEADASS]{{\"schema\":1,\"event\":\"death\",\"mod_version\":\"0.1.0\",\"session_id\":\"s\",\"sequence\":9,\"client_time_ms\":11}}"
    )
    .unwrap();

    let event = tokio::time::timeout(Duration::from_secs(2), receiver.recv())
        .await
        .expect("event arrives")
        .expect("channel open");
    assert_eq!(event.kind, EventKind::Death);
    assert_eq!(event.sequence, 9);

    worker.abort();
    std::fs::remove_file(&path).ok();
}
