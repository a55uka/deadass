use deadass_shared::{EventKind, EventSource, GameEvent};
use serde::Deserialize;
use std::path::PathBuf;
use std::time::Duration;
use tokio::sync::mpsc;

pub const BRIDGE_RECORD_PREFIX: &str = "[DEADASS]";
pub const BRIDGE_SCHEMA: u32 = 1;

const TAIL_POLL: Duration = Duration::from_millis(20);
const READ_CHUNK: usize = 16 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModSignal {
    HookReady,
    Game(GameEvent),
}

#[derive(Deserialize)]
struct WireRecord {
    schema: u32,
    #[allow(dead_code)]
    session_id: String,
    sequence: u64,
    client_time_ms: u64,
    #[serde(flatten)]
    kind: WireKind,
}

#[derive(Deserialize)]
#[serde(tag = "event")]
enum WireKind {
    #[serde(rename = "hook_ready")]
    HookReady {},
    #[serde(rename = "kill")]
    Kill {},
    #[serde(rename = "death")]
    Death {},
    #[serde(rename = "assist")]
    Assist {},
    #[serde(rename = "respawn")]
    Respawn {},
    #[serde(rename = "ability_used")]
    AbilityUsed { ability_slot: u8 },
    #[serde(rename = "ability_ready")]
    AbilityReady { ability_slot: u8 },
}

pub fn parse_bridge_line(line: &str) -> Option<ModSignal> {
    let prefix_at = line.find(BRIDGE_RECORD_PREFIX)?;
    let record: WireRecord =
        serde_json::from_str(&line[prefix_at + BRIDGE_RECORD_PREFIX.len()..]).ok()?;
    if record.schema != BRIDGE_SCHEMA {
        return None;
    }
    let stamped = |kind| {
        ModSignal::Game(GameEvent::new(
            record.sequence,
            record.client_time_ms,
            EventSource::Mod,
            kind,
        ))
    };
    match record.kind {
        WireKind::HookReady {} => Some(ModSignal::HookReady),
        WireKind::Kill {} => Some(stamped(EventKind::Kill)),
        WireKind::Death {} => Some(stamped(EventKind::Death)),
        WireKind::Assist {} => Some(stamped(EventKind::Assist)),
        WireKind::Respawn {} => Some(stamped(EventKind::Respawn)),
        WireKind::AbilityUsed { ability_slot } => {
            Some(stamped(EventKind::AbilityUsed { slot: ability_slot }))
        }
        WireKind::AbilityReady { ability_slot } => {
            Some(stamped(EventKind::AbilityReady { slot: ability_slot }))
        }
    }
}

pub struct LogTail {
    path: PathBuf,
    sender: mpsc::UnboundedSender<GameEvent>,
}

impl LogTail {
    pub fn new(path: PathBuf, sender: mpsc::UnboundedSender<GameEvent>) -> Self {
        Self { path, sender }
    }

    pub async fn run(self) {
        let mut offset = end_offset(&self.path);
        let mut pending = Vec::new();
        loop {
            offset = drain_new_lines(&self.path, &mut pending, offset, &self.sender);
            tokio::time::sleep(TAIL_POLL).await;
        }
    }
}

fn end_offset(path: &PathBuf) -> u64 {
    std::fs::metadata(path).map(|meta| meta.len()).unwrap_or(0)
}

fn drain_new_lines(
    path: &PathBuf,
    pending: &mut Vec<u8>,
    offset: u64,
    sender: &mpsc::UnboundedSender<GameEvent>,
) -> u64 {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(_) => return offset,
    };
    let length = file.metadata().map(|meta| meta.len()).unwrap_or(offset);
    let mut cursor = if length < offset { 0 } else { offset };
    if file.seek(SeekFrom::Start(cursor)).is_err() {
        return cursor;
    }
    let mut chunk = vec![0u8; READ_CHUNK];
    loop {
        let read = match file.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(read) => read,
        };
        pending.extend_from_slice(&chunk[..read]);
        cursor += read as u64;
        while let Some(end) = pending.iter().position(|byte| *byte == b'\n') {
            let line: Vec<u8> = pending.drain(..=end).collect();
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
    cursor
}

#[cfg(test)]
mod bridge_line_parsing {
    use super::*;

    fn record(event: &str) -> String {
        format!(
            "[DEADASS]{{\"schema\":1,\"event\":\"{event}\",\"mod_version\":\"0.1.0\",\"session_id\":\"abc\",\"sequence\":7,\"client_time_ms\":4242}}"
        )
    }

    #[test]
    fn kill_maps_to_mod_game_event() {
        let Some(ModSignal::Game(event)) = parse_bridge_line(&record("kill")) else {
            panic!("kill line must parse");
        };
        assert_eq!(event.source, EventSource::Mod);
        assert_eq!(event.kind, EventKind::Kill);
        assert_eq!(event.sequence, 7);
        assert_eq!(event.wall_time_ms, 4242);
    }

    #[test]
    fn hook_ready_is_not_a_game_event() {
        assert_eq!(
            parse_bridge_line(&record("hook_ready")),
            Some(ModSignal::HookReady)
        );
    }

    #[test]
    fn mismatched_schema_is_rejected() {
        let line = "[DEADASS]{\"schema\":999,\"event\":\"kill\",\"session_id\":\"abc\",\"sequence\":1,\"client_time_ms\":1}";
        assert_eq!(parse_bridge_line(line), None);
    }

    #[test]
    fn console_log_wrapping_does_not_break_prefix_search() {
        let line = format!("[123.45] Panorama: {}", record("death"));
        let Some(ModSignal::Game(event)) = parse_bridge_line(&line) else {
            panic!("wrapped line must parse");
        };
        assert_eq!(event.kind, EventKind::Death);
    }
}
