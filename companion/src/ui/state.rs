use crate::toys::{ConnectionMode, ToyDevice};
use deadass_shared::AppConfig;
use std::collections::VecDeque;
use std::fmt;
use std::time::{Duration, Instant};

const SOURCE_FRESH: Duration = Duration::from_secs(5);
const INITIAL_LOG: &str = "waiting for deadlock";

pub const MAX_LOG_LINES: usize = 200;

#[derive(Debug, Clone, Default)]
pub struct SourceStatus {
    pub mod_seen: Option<Instant>,
}

impl SourceStatus {
    pub fn is_live(&self) -> bool {
        self.mod_seen.is_some_and(|at| at.elapsed() < SOURCE_FRESH)
    }

    pub fn mark_seen(&mut self) {
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

impl fmt::Display for LogPhase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing => write!(f, "missing"),
            Self::Waiting => write!(f, "waiting"),
            Self::Tailing => write!(f, "tailing"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ToyStatus {
    pub mode: ConnectionMode,
    pub device_names: Vec<String>,
    pub error: Option<String>,
}

impl ToyStatus {
    fn disconnected() -> Self {
        Self {
            mode: ConnectionMode::Disconnected,
            device_names: Vec::new(),
            error: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AppState {
    pub config: AppConfig,
    pub sources: SourceStatus,
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
            sources: SourceStatus::default(),
            toys: ToyStatus::disconnected(),
            last_log: String::from(INITIAL_LOG),
            log_lines: VecDeque::from([String::from(INITIAL_LOG)]),
            log_phase: LogPhase::Missing,
            log_path: None,
        }
    }

    pub fn mark_source_seen(&mut self) {
        self.sources.mark_seen();
    }

    pub fn set_toys(&mut self, mode: ConnectionMode, devices: &[ToyDevice]) {
        self.toys.mode = mode;
        self.toys.device_names = ToyDevice::names(devices);
        self.toys.error = None;
    }

    pub fn set_toy_error(&mut self, error: String) {
        self.toys.error = Some(error);
    }

    pub fn is_tailing(&self) -> bool {
        self.log_phase == LogPhase::Tailing
    }

    pub fn push_log(&mut self, line: impl Into<String>) {
        let line = line.into();
        self.last_log = line.clone();
        self.log_lines.push_back(line);
        while self.log_lines.len() > MAX_LOG_LINES {
            self.log_lines.pop_front();
        }
    }

    pub fn set_tailing(&mut self, path: impl Into<String>, line: impl Into<String>) {
        self.log_phase = LogPhase::Tailing;
        self.log_path = Some(path.into());
        self.push_log(line);
    }

    pub fn set_waiting(&mut self, path: impl Into<String>, line: impl Into<String>) {
        self.log_phase = LogPhase::Waiting;
        self.log_path = Some(path.into());
        self.push_log(line);
    }

    pub fn set_missing(&mut self, line: impl Into<String>) {
        self.log_phase = LogPhase::Missing;
        self.log_path = None;
        self.push_log(line);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_caps_and_tracks_last_line() {
        let mut state = AppState::new(AppConfig::default());
        assert_eq!(state.log_lines.len(), 1);
        for i in 0..(MAX_LOG_LINES + 10) {
            state.push_log(format!("line {i}"));
        }
        assert_eq!(state.log_lines.len(), MAX_LOG_LINES);
        assert_eq!(state.last_log, format!("line {}", MAX_LOG_LINES + 9));
        assert_eq!(state.log_lines.back().unwrap(), &state.last_log);
        assert_eq!(state.log_lines.front().unwrap(), "line 10");
    }
}
