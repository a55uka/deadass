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
        open_sink()
    })
    .as_ref()
}

/// The gated sink caches its disabled state, so unconditional logging uses
/// its own slot; both append to the same file.
static FORCED: OnceLock<Option<Mutex<std::fs::File>>> = OnceLock::new();

fn force_sink() -> Option<&'static Mutex<std::fs::File>> {
    FORCED.get_or_init(open_sink).as_ref()
}

fn open_sink() -> Option<Mutex<std::fs::File>> {
    let dir = super::offsets::dll_directory().unwrap_or_else(|| std::path::PathBuf::from("."));
    let path = dir.join("deadass_dll.log");
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .ok()
        .map(Mutex::new)
}

/// Marker-gated logging (chatty, per-tick diagnostics).
pub fn log(line: &str) {
    write(sink(), line);
}

/// Unconditional logging for one-shot session diagnostics (scan results,
/// schema outcomes) — always lands in deadass_dll.log next to the DLL, no
/// marker file needed.
pub fn always(line: &str) {
    write(force_sink(), line);
}

fn write(sink: Option<&'static Mutex<std::fs::File>>, line: &str) {
    let Some(file) = sink else {
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
