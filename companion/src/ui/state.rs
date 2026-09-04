use crate::toys::ConnectionMode;
use deadass_shared::{AppConfig, InputMode};

#[derive(Debug, Clone)]
pub struct InputSourceStatus {
    pub mod_connected: bool,
    pub dll_connected: bool,
    pub external_active: bool,
}

#[derive(Debug, Clone)]
pub struct ToyStatus {
    pub mode: ConnectionMode,
    pub device_names: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct AppState {
    pub config: AppConfig,
    pub sources: InputSourceStatus,
    pub toys: ToyStatus,
    pub last_log: String,
}

impl AppState {
    pub fn new(config: AppConfig) -> Self {
        Self {
            config,
            sources: InputSourceStatus {
                mod_connected: false,
                dll_connected: false,
                external_active: false,
            },
            toys: ToyStatus {
                mode: ConnectionMode::Disconnected,
                device_names: Vec::new(),
            },
            last_log: String::from("waiting for deadlock"),
        }
    }

    pub fn input_mode(&self) -> InputMode {
        self.config.input_mode
    }

    pub fn set_input_mode(&mut self, mode: InputMode) {
        self.config.input_mode = mode;
    }

    pub fn log(&mut self, line: impl Into<String>) {
        self.last_log = line.into();
    }
}
