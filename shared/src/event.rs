use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventKind {
    Kill,
    Death,
    Assist,
    AbilityUsed { slot: u8 },
    AbilityReady { slot: u8 },
    Respawn,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GameEvent {
    pub sequence: u64,
    pub wall_time_ms: u64,
    pub kind: EventKind,
}

impl GameEvent {
    pub fn new(sequence: u64, wall_time_ms: u64, kind: EventKind) -> Self {
        Self {
            sequence,
            wall_time_ms,
            kind,
        }
    }
}
