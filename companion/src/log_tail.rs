use crate::bridge::{ModSignal, parse_bridge_line};
use crate::ui::AppState;
use deadass_shared::GameEvent;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, mpsc};

const TAIL_POLL: Duration = Duration::from_millis(20);
const WAIT_POLL: Duration = Duration::from_millis(500);
const READ_CHUNK: usize = 16 * 1024;

pub struct LogTail {
    path: PathBuf,
    sender: mpsc::UnboundedSender<GameEvent>,
    state: Option<Arc<Mutex<AppState>>>,
}

impl LogTail {
    pub fn new(path: PathBuf, sender: mpsc::UnboundedSender<GameEvent>) -> Self {
        Self {
            path,
            sender,
            state: None,
        }
    }

    pub fn with_state(
        path: PathBuf,
        sender: mpsc::UnboundedSender<GameEvent>,
        state: Arc<Mutex<AppState>>,
    ) -> Self {
        Self {
            path,
            sender,
            state: Some(state),
        }
    }

    pub async fn run(self) {
        let mut tail = TailCursor::fresh();
        let mut waiting_announced = false;
        loop {
            if self.path.is_file() {
                waiting_announced = false;
                tail.baseline_once(&self.path, &self.state).await;
                tail.drain_available(&self.path, &self.sender);
                tokio::time::sleep(TAIL_POLL).await;
            } else {
                tail.reset();
                if !waiting_announced {
                    self.report_waiting().await;
                    waiting_announced = true;
                }
                tokio::time::sleep(WAIT_POLL).await;
            }
        }
    }

    async fn report_waiting(&self) {
        if let Some(state) = &self.state {
            let path = self.path.display().to_string();
            state.lock().await.set_waiting(
                path.clone(),
                format!(
                    "waiting for console.log to be created; add -condebug to deadlock launch options: {path}"
                ),
            );
        }
    }
}

struct TailCursor {
    offset: Option<u64>,
    pending: Vec<u8>,
}

impl TailCursor {
    fn fresh() -> Self {
        Self {
            offset: None,
            pending: Vec::new(),
        }
    }

    fn reset(&mut self) {
        self.offset = None;
        self.pending.clear();
    }

    async fn baseline_once(&mut self, path: &PathBuf, state: &Option<Arc<Mutex<AppState>>>) {
        if self.offset.is_some() {
            return;
        }
        let length = std::fs::metadata(path).map(|meta| meta.len()).unwrap_or(0);
        self.offset = Some(length);
        self.pending.clear();
        if let Some(state) = state {
            let path = path.display().to_string();
            state
                .lock()
                .await
                .set_tailing(path.clone(), format!("tailing {path}"));
        }
    }

    fn drain_available(&mut self, path: &PathBuf, sender: &mpsc::UnboundedSender<GameEvent>) {
        use std::io::{Read, Seek, SeekFrom};
        let mut file = match std::fs::File::open(path) {
            Ok(file) => file,
            Err(_) => return,
        };
        let length = file.metadata().map(|meta| meta.len()).unwrap_or(0);
        let mut cursor = match self.offset {
            Some(offset) if length >= offset => offset,
            _ => 0,
        };
        if file.seek(SeekFrom::Start(cursor)).is_err() {
            self.offset = Some(cursor);
            return;
        }
        let mut chunk = vec![0u8; READ_CHUNK];
        loop {
            let read = match file.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(read) => read,
            };
            self.pending.extend_from_slice(&chunk[..read]);
            cursor += read as u64;
            while let Some(end) = self.pending.iter().position(|byte| *byte == b'\n') {
                let line: Vec<u8> = self.pending.drain(..=end).collect();
                if let Ok(text) = std::str::from_utf8(&line)
                    && let Some(ModSignal::Game(event)) = parse_bridge_line(text)
                {
                    let _ = sender.send(event);
                }
            }
            if read < READ_CHUNK {
                break;
            }
        }
        self.offset = Some(cursor);
    }
}
