use std::fs::OpenOptions;
use std::io::Write;
use std::sync::{Mutex, OnceLock};

const ENV_SWITCH: &str = "DEADASS_DLL_DEBUG";
const FILE_SWITCH: &str = "deadass_dll_debug.txt";

static SINK: OnceLock<Option<Mutex<std::fs::File>>> = OnceLock::new();

fn sink() -> Option<&'static Mutex<std::fs::File>> {
    SINK.get_or_init(|| {
        let env_on = std::env::var(ENV_SWITCH)
            .map(|raw| !raw.is_empty() && raw != "0")
            .unwrap_or(false);
        let dir = super::offsets::dll_directory().unwrap_or_else(|| std::path::PathBuf::from("."));
        let file_on = dir.join(FILE_SWITCH).is_file();
        if !env_on && !file_on {
            return None;
        }
        let path = dir.join("deadass_dll.log");
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .ok()
            .map(Mutex::new)
    })
    .as_ref()
}

pub fn log(line: &str) {
    let Some(file) = sink() else {
        return;
    };
    let Ok(mut handle) = file.lock() else {
        return;
    };
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_millis())
        .unwrap_or(0);
    let _ = writeln!(handle, "[{timestamp}] {line}");
}
