use crate::toys::{ConnectionMode, ToyDevice};
use deadass_shared::{AppConfig, EventSource, InputMode};
use std::time::{Duration, Instant};

const SOURCE_FRESH: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Default)]
pub struct InputSourceStatus {
    pub mod_seen: Option<Instant>,
    pub dll_seen: Option<Instant>,
    pub external_seen: Option<Instant>,
}

impl InputSourceStatus {
    fn fresh(seen: Option<Instant>) -> bool {
        seen.is_some_and(|at| at.elapsed() < SOURCE_FRESH)
    }

    pub fn mod_active(&self) -> bool {
        Self::fresh(self.mod_seen)
    }

    pub fn dll_active(&self) -> bool {
        Self::fresh(self.dll_seen)
    }

    pub fn external_active(&self) -> bool {
        Self::fresh(self.external_seen)
    }

    pub fn note(&mut self, source: EventSource) {
        let slot = match source {
            EventSource::Mod => &mut self.mod_seen,
            EventSource::Dll => &mut self.dll_seen,
            EventSource::External => &mut self.external_seen,
        };
        *slot = Some(Instant::now());
    }
}

#[derive(Debug, Clone)]
pub struct ToyStatus {
    pub mode: ConnectionMode,
    pub device_names: Vec<String>,
    pub error: Option<String>,
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
            sources: InputSourceStatus::default(),
            toys: ToyStatus {
                mode: ConnectionMode::Disconnected,
                device_names: Vec::new(),
                error: None,
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

    pub fn note_source(&mut self, source: EventSource) {
        self.sources.note(source);
    }

    pub fn note_toys(&mut self, mode: ConnectionMode, devices: &[ToyDevice]) {
        self.toys.mode = mode;
        self.toys.device_names = devices.iter().map(|device| device.name.clone()).collect();
        self.toys.error = None;
    }

    pub fn note_toy_error(&mut self, error: String) {
        self.toys.error = Some(error);
    }

    pub fn log(&mut self, line: impl Into<String>) {
        self.last_log = line.into();
    }
}
