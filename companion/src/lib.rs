pub mod bridge;
pub mod config_store;
pub mod game_log;
pub mod haptics;
pub mod pipeline;
pub mod servers;
pub mod toys;
pub mod transport;
pub mod ui;

pub use bridge::{BRIDGE_RECORD_PREFIX, BRIDGE_SCHEMA, LogTail, ModSignal, parse_bridge_line};
pub use config_store::{ConfigStore, default_config_path};
pub use game_log::{ConsoleLogLocation, discover_console_log};
pub use haptics::{GateDecision, HapticCommand, HapticGate, SuppressReason, decide_haptic, resolve_haptic};
pub use pipeline::{Pipeline, disconnect, load_config, start};
pub use toys::{ConnectionMode, ToyDevice, ToyError, ToyHub};
pub use transport::{EventBus, EventDeduplicator};
