use std::path::PathBuf;

pub const DEADLOCK_APP_ID: u32 = 1422450;
const CONSOLE_LOG_RELATIVE: &str = "game/citadel/console.log";
const CITADEL_DIR_RELATIVE: &str = "game/citadel";

#[derive(Debug, Clone)]
pub struct ConsoleLogLocation {
    pub path: PathBuf,
    pub already_created: bool,
}

pub fn discover_console_log() -> Option<ConsoleLogLocation> {
    let install = steam_library_roots()
        .into_iter()
        .find_map(deadlock_install)?;
    let path = install.join(CONSOLE_LOG_RELATIVE);
    Some(ConsoleLogLocation {
        already_created: path.is_file(),
        path,
    })
}

fn steam_library_roots() -> Vec<PathBuf> {
    steamlocate::locate_all()
        .as_ref()
        .map(|roots| roots.iter().map(|root| root.path().to_owned()).collect())
        .unwrap_or_default()
}

fn deadlock_install(steam_root: PathBuf) -> Option<PathBuf> {
    let libraries = steamlocate::SteamDir::from_dir(&steam_root)
        .ok()?
        .library_paths()
        .ok()?;
    let install_dir = libraries.into_iter().find_map(|library| {
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
    install_dir
        .join(CITADEL_DIR_RELATIVE)
        .is_dir()
        .then_some(install_dir)
}
