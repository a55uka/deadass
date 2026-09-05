pub mod config;
pub mod event;

pub use config::{AppConfig, InputMode, Pattern, TriggerConfig, TriggerKind};
pub use event::{EventKind, EventSource, GameEvent};

pub fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0)
}
