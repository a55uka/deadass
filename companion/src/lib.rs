pub mod bridge;
pub mod config_store;
pub mod config_watch;
pub mod dedup;
pub mod dll_server;
pub mod game_log;
pub mod haptics;
pub mod injector;
pub mod log_tail;
pub mod openshock;
pub mod pipeline;
pub mod servers;
pub mod toys;
pub mod transport;
pub mod ui;

pub use bridge::{BRIDGE_RECORD_PREFIX, BRIDGE_SCHEMA, ModSignal, parse_bridge_line};
pub use config_store::{ConfigStore, default_config_path};
pub use dedup::EventDeduplicator;
pub use dll_server::DllEventServer;
pub use game_log::{ConsoleLogLocation, discover_console_log};
pub use haptics::{GateDecision, HapticCommand, HapticGate, SuppressReason};
pub use injector::{DLL_FILE_NAME, GAME_PROCESS, resolve_dll_path, spawn_supervisor};
pub use log_tail::LogTail;
pub use openshock::{ControlCommand, OpenShockClient};
pub use pipeline::{
    Pipeline, SourceGate, disconnect, load_config, set_source, start,
};
pub use toys::{ConnectionMode, ToyDevice, ToyError, ToyHub};
pub use transport::{EventBus, EventIngress, EventOutlet};
