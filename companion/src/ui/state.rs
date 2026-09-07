use crate::toys::{ConnectionMode, ToyDevice};
use deadass_shared::AppConfig;
use std::collections::VecDeque;
use std::time::{Duration, Instant};

const SOURCE_FRESH: Duration = Duration::from_secs(5);

/// How many log lines the UI history keeps.
pub const MAX_LOG_LINES: usize = 200;

#[derive(Debug, Clone, Default)]
pub struct InputSourceStatus {
    pub mod_seen: Option<Instant>,
}

impl InputSourceStatus {
    fn fresh(seen: Option<Instant>) -> bool {
        seen.is_some_and(|at| at.elapsed() < SOURCE_FRESH)
    }

    pub fn mod_active(&self) -> bool {
        Self::fresh(self.mod_seen)
    }

    pub fn note(&mut self) {
        self.mod_seen = Some(Instant::now());
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LogPhase {
    #[default]
    Missing,
    Waiting,
    Tailing,
}

impl LogPhase {
    pub fn as_str(self) -> &'static str {
        match self {
            LogPhase::Missing => "missing",
            LogPhase::Waiting => "waiting",
            LogPhase::Tailing => "tailing",
        }
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
    pub log_lines: VecDeque<String>,
    pub log_phase: LogPhase,
    pub log_path: Option<String>,
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
            log_lines: VecDeque::from([String::from("waiting for deadlock")]),
            log_phase: LogPhase::Missing,
            log_path: None,
        }
    }

    pub fn note_source(&mut self) {
        self.sources.note();
    }

    pub fn note_toys(&mut self, mode: ConnectionMode, devices: &[ToyDevice]) {
        self.toys.mode = mode;
        self.toys.device_names = devices.iter().map(|device| device.name.clone()).collect();
        self.toys.error = None;
    }

    pub fn note_toy_error(&mut self, error: String) {
        self.toys.error = Some(error);
    }

    pub fn log_tailing(&self) -> bool {
        self.log_phase == LogPhase::Tailing
    }

    pub fn log(&mut self, line: impl Into<String>) {
        let line = line.into();
        self.last_log = line.clone();
        self.log_lines.push_back(line);
        while self.log_lines.len() > MAX_LOG_LINES {
            self.log_lines.pop_front();
        }
    }

    pub fn note_log_tail(&mut self, path: impl Into<String>, line: impl Into<String>) {
        self.log_phase = LogPhase::Tailing;
        self.log_path = Some(path.into());
        self.log(line);
    }

    pub fn note_log_waiting(&mut self, path: impl Into<String>, line: impl Into<String>) {
        self.log_phase = LogPhase::Waiting;
        self.log_path = Some(path.into());
        self.log(line);
    }

    pub fn note_log_missing(&mut self, line: impl Into<String>) {
        self.log_phase = LogPhase::Missing;
        self.log_path = None;
        self.log(line);
    }
}

#[cfg(test)]
mod log_history {
    use super::*;
    use deadass_shared::AppConfig;

    #[test]
    fn history_caps_and_tracks_last_line() {
        let mut state = AppState::new(AppConfig::default());
        assert_eq!(state.log_lines.len(), 1);
        for i in 0..(MAX_LOG_LINES + 10) {
            state.log(format!("line {i}"));
        }
        assert_eq!(state.log_lines.len(), MAX_LOG_LINES);
        assert_eq!(state.last_log, format!("line {}", MAX_LOG_LINES + 9));
        assert_eq!(state.log_lines.back().unwrap(), &state.last_log);
        assert_eq!(state.log_lines.front().unwrap(), "line 10");
    }
}
