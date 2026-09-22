use deadass_companion::log_tail::LogTail;
use deadass_companion::pipeline::SourceGate;
use deadass_shared::{DataSource, EventKind};
use std::io::Write;
use std::time::Duration;

#[tokio::test]
async fn tail_delivers_mod_lines_as_game_events() {
    let path = std::env::temp_dir().join(format!("deadass-tail-{}.log", std::process::id()));
    std::fs::write(&path, "old line\n").unwrap();

    let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
    let worker =
        tokio::spawn(LogTail::new(path.clone(), sender, SourceGate::new(DataSource::Mod)).run());
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
async fn tail_drops_non_kill_events_when_dll_source_is_selected() {
    let path = std::env::temp_dir().join(format!("deadass-tail-gate-{}.log", std::process::id()));
    std::fs::write(&path, "").unwrap();

    let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
    let worker =
        tokio::spawn(LogTail::new(path.clone(), sender, SourceGate::new(DataSource::Dll)).run());
    tokio::time::sleep(Duration::from_millis(50)).await;

    let mut log = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap();
    // Death must be gated off (the DLL owns it), but kill passes through as
    // the hybrid rule: kill/assist detection stays with the addon.
    writeln!(
        log,
        "[DEADASS]{{\"schema\":1,\"event\":\"death\",\"mod_version\":\"0.1.0\",\"session_id\":\"s\",\"sequence\":5,\"client_time_ms\":5}}"
    )
    .unwrap();
    writeln!(
        log,
        "[DEADASS]{{\"schema\":1,\"event\":\"kill\",\"mod_version\":\"0.1.0\",\"session_id\":\"s\",\"sequence\":6,\"client_time_ms\":6}}"
    )
    .unwrap();

    // Only the kill may arrive.
    let event = tokio::time::timeout(Duration::from_secs(2), receiver.recv())
        .await
        .expect("kill arrives")
        .expect("channel open");
    assert_eq!(event.kind, EventKind::Kill);
    assert_eq!(event.sequence, 6);

    let outcome = tokio::time::timeout(Duration::from_millis(300), receiver.recv()).await;
    assert!(
        outcome.is_err(),
        "mod death events must be gated off for dll source"
    );

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
    let worker = tokio::spawn(
        LogTail::with_state(
            path.clone(),
            sender,
            SourceGate::new(DataSource::Mod),
            state.clone(),
        )
        .run(),
    );

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
