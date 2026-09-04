pub mod config;
pub mod event;

pub use config::{AppConfig, InputMode, Pattern, TriggerConfig, TriggerKind};
pub use event::{EventKind, EventSource, GameEvent};
