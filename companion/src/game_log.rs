use std::path::PathBuf;

pub const DEADLOCK_APP_ID: u32 = 1422450;

#[derive(Debug, Clone)]
pub struct ConsoleLogLocation {
    pub path: PathBuf,
    pub already_created: bool,
}

pub fn discover_console_log() -> Option<ConsoleLogLocation> {
    let install = steam_roots().into_iter().find_map(deadlock_install)?;
    let path = install.join("game/citadel/console.log");
    let already_created = path.is_file();
    Some(ConsoleLogLocation {
        path,
        already_created,
    })
}

fn steam_roots() -> Vec<PathBuf> {
    steamlocate::locate_all()
        .as_ref()
        .map(|dirs| dirs.iter().map(|dir| dir.path().to_owned()).collect())
        .unwrap_or_default()
}

fn deadlock_install(root: PathBuf) -> Option<PathBuf> {
    let steam_dir = steamlocate::SteamDir::from_dir(&root).ok()?;
    let libraries = steam_dir.library_paths().ok()?;
    let install = libraries.into_iter().find_map(|library| {
        let app = steamlocate::Library::from_dir(&library)
            .ok()?
            .app(DEADLOCK_APP_ID)?;
        let app = app.ok()?;
        (app.app_id == DEADLOCK_APP_ID).then(|| {
            library
                .join("steamapps")
                .join("common")
                .join(&app.install_dir)
        })
    })?;
    install.join("game/citadel").is_dir().then_some(install)
}
