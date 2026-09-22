use deadass_companion::config_watch;
use deadass_companion::ui::AppState;
use deadass_shared::AppConfig;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

fn temp_config_path(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "deadass-watch-{tag}-{}-{}.toml",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|nanos| nanos.as_nanos())
            .unwrap_or(0)
    ))
}

async fn wait_for(condition: impl Fn(&AppState) -> bool, state: &Arc<Mutex<AppState>>) -> bool {
    for _ in 0..40 {
        // Lock and drop each iteration so the watcher can write in between.
        if condition(&*state.lock().await) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    condition(&*state.lock().await)
}

/// The watcher detects changes by mtime, and Windows timestamps can lag the
/// clock; nudge the file's mtime so a rewrite is always observed.
fn force_mtime_change(path: &PathBuf) {
    let file = std::fs::OpenOptions::new()
        .write(true)
        .open(path)
        .expect("file exists");
    file.set_modified(std::time::SystemTime::now() + Duration::from_millis(50))
        .expect("mtime can be set");
}

#[tokio::test]
async fn watcher_reloads_edited_file() {
    let path = temp_config_path("reload");
    std::fs::write(
        &path,
        toml::to_string_pretty(&AppConfig::default()).unwrap(),
    )
    .unwrap();

    let state = Arc::new(tokio::sync::Mutex::new(AppState::new(AppConfig::default())));
    let handle = config_watch::spawn(path.clone(), state.clone());

    let mut edited = AppConfig::default();
    edited.master_gain = 0.25;
    std::fs::write(&path, toml::to_string_pretty(&edited).unwrap()).unwrap();
    force_mtime_change(&path);

    let reloaded = wait_for(|state| state.config.master_gain == 0.25, &state).await;
    assert!(reloaded, "watcher should pick up the edited file");
    {
        let locked = state.lock().await;
        assert_eq!(locked.config_rev, 2, "reload bumps the revision");
        assert!(
            locked
                .log_lines
                .iter()
                .any(|line| line.contains("master_gain 1.00 -> 0.25")),
            "reload logs the change, got {:?}",
            locked.log_lines
        );
    }

    handle.abort();
    std::fs::remove_file(&path).ok();
}

#[tokio::test]
async fn watcher_keeps_running_config_on_invalid_file() {
    let path = temp_config_path("invalid");
    std::fs::write(
        &path,
        toml::to_string_pretty(&AppConfig::default()).unwrap(),
    )
    .unwrap();

    let state = Arc::new(tokio::sync::Mutex::new(AppState::new(AppConfig::default())));
    let handle = config_watch::spawn(path.clone(), state.clone());

    std::fs::write(&path, "not valid toml [[[").unwrap();
    force_mtime_change(&path);
    tokio::time::sleep(Duration::from_millis(1200)).await;

    let locked = state.lock().await;
    assert_eq!(locked.config.master_gain, AppConfig::default().master_gain);
    assert!(
        locked
            .log_lines
            .iter()
            .any(|line| line.contains("could not be parsed")),
        "parse failure is logged, got {:?}",
        locked.log_lines
    );
    drop(locked);

    handle.abort();
    std::fs::remove_file(&path).ok();
}

#[tokio::test]
async fn watcher_skips_identical_config() {
    let path = temp_config_path("identical");
    std::fs::write(
        &path,
        toml::to_string_pretty(&AppConfig::default()).unwrap(),
    )
    .unwrap();

    let state = Arc::new(tokio::sync::Mutex::new(AppState::new(AppConfig::default())));
    let handle = config_watch::spawn(path.clone(), state.clone());

    // Rewrite the same content: mtime moves but nothing changed.
    std::fs::write(
        &path,
        toml::to_string_pretty(&AppConfig::default()).unwrap(),
    )
    .unwrap();
    tokio::time::sleep(Duration::from_millis(1200)).await;

    let locked = state.lock().await;
    assert_eq!(
        locked.config_rev, 1,
        "identical config does not bump the rev"
    );
    assert!(
        !locked
            .log_lines
            .iter()
            .any(|line| line.contains("config reloaded")),
        "identical config is not logged as a reload"
    );

    handle.abort();
    std::fs::remove_file(&path).ok();
}
