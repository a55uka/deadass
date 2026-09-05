use std::path::PathBuf;

pub const DEADLOCK_APP_ID: u32 = 1_422_450;

#[derive(Debug, Clone)]
pub struct ConsoleLogLocation {
    pub path: PathBuf,
    pub already_created: bool,
}

pub fn discover_console_log() -> Option<ConsoleLogLocation> {
    let mut found: Vec<ConsoleLogLocation> = steam_roots()
        .into_iter()
        .flat_map(deadlock_installs)
        .map(|install| ConsoleLogLocation {
            path: install.join("game/citadel/console.log"),
            already_created: false,
        })
        .collect();
    for location in &mut found {
        location.already_created = location.path.is_file();
    }
    found
        .into_iter()
        .max_by_key(|location| (location.already_created, location.path.clone()))
}

fn steam_roots() -> Vec<PathBuf> {
    steamlocate::locate_all()
        .as_ref()
        .map(|dirs| dirs.iter().map(|dir| dir.path().to_owned()).collect())
        .unwrap_or_default()
}

fn deadlock_installs(root: PathBuf) -> Vec<PathBuf> {
    let Ok(steam_dir) = steamlocate::SteamDir::from_dir(&root) else {
        return Vec::new();
    };
    let Ok(libraries) = steam_dir.library_paths() else {
        return Vec::new();
    };
    libraries
        .into_iter()
        .filter_map(|library| {
            let app = steamlocate::Library::from_dir(&library)
                .ok()?
                .app(DEADLOCK_APP_ID)?;
            let app = app.ok()?;
            (app.app_id == DEADLOCK_APP_ID).then(|| library.join("steamapps").join("common").join(&app.install_dir))
        })
        .filter(|install| install.join("game/citadel").is_dir())
        .collect()
}

#[cfg(test)]
mod console_log_selection {
    use super::*;

    fn located(path: &str, created: bool) -> ConsoleLogLocation {
        ConsoleLogLocation {
            path: PathBuf::from(path),
            already_created: created,
        }
    }

    fn newest(locations: Vec<ConsoleLogLocation>) -> Option<ConsoleLogLocation> {
        locations
            .into_iter()
            .max_by_key(|location| (location.already_created, location.path.clone()))
    }

    #[test]
    fn existing_log_beats_missing_log() {
        let picked = newest(vec![
            located("/lib-a/game/citadel/console.log", false),
            located("/lib-b/game/citadel/console.log", true),
        ]);
        assert_eq!(
            picked.map(|found| found.path),
            Some(PathBuf::from("/lib-b/game/citadel/console.log"))
        );
    }
}
