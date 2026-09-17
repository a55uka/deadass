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
    pub dll_seen: Option<Instant>,
}

impl SourceStatus {
    pub fn is_mod_live(&self) -> bool {
        self.mod_seen.is_some_and(|at| at.elapsed() < SOURCE_FRESH)
    }

    pub fn is_dll_live(&self) -> bool {
        self.dll_seen.is_some_and(|at| at.elapsed() < SOURCE_FRESH)
    }

    pub fn mark_mod_seen(&mut self) {
        self.mod_seen = Some(Instant::now());
    }

    pub fn mark_dll_seen(&mut self) {
        self.dll_seen = Some(Instant::now());
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum InjectPhase {
    /// Mod source selected (or the DLL supervisor has nothing to report).
    #[default]
    Idle,
    /// Dll source selected but deadlock.exe is not running.
    WaitingForGame,
    /// deadass-dll.dll is loaded in the game.
    Injected,
    /// The last injection attempt failed.
    Failed,
}

impl fmt::Display for InjectPhase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Idle => write!(f, "idle"),
            Self::WaitingForGame => write!(f, "waiting for game"),
            Self::Injected => write!(f, "injected"),
            Self::Failed => write!(f, "failed"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InjectStatus {
    pub phase: InjectPhase,
    pub detail: Option<String>,
}

impl InjectStatus {
    pub fn phase(phase: InjectPhase) -> Self {
        Self {
            phase,
            detail: None,
        }
    }

    pub fn detail(phase: InjectPhase, detail: impl Into<String>) -> Self {
        Self {
            phase,
            detail: Some(detail.into()),
        }
    }

    pub fn failed(detail: impl Into<String>) -> Self {
        Self {
            phase: InjectPhase::Failed,
            detail: Some(detail.into()),
        }
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
    /// Bumped whenever the config changes
    pub config_rev: u64,
    pub sources: SourceStatus,
    pub inject: InjectStatus,
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
            config_rev: 1,
            sources: SourceStatus::default(),
            inject: InjectStatus::default(),
            toys: ToyStatus::disconnected(),
            last_log: String::from(INITIAL_LOG),
            log_lines: VecDeque::from([String::from(INITIAL_LOG)]),
            log_phase: LogPhase::Missing,
            log_path: None,
        }
    }
    
    pub fn replace_config(&mut self, config: AppConfig) -> AppConfig {
        let previous = self.config.clone();
        self.config = config;
        self.config_rev = self.config_rev.wrapping_add(1);
        previous
    }

    pub fn set_inject(&mut self, phase: InjectPhase, detail: Option<String>) {
        self.inject = InjectStatus {
            phase,
            detail,
        };
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
