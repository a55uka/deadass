use deadass_shared::{EventKind, GameEvent};
use serde::Deserialize;

pub const BRIDGE_RECORD_PREFIX: &str = "[DEADASS]";
pub const BRIDGE_SCHEMA: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModSignal {
    HookReady,
    Game(GameEvent),
}

#[derive(Deserialize)]
struct WireRecord {
    schema: u32,
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
    let payload_at = line.find(BRIDGE_RECORD_PREFIX)? + BRIDGE_RECORD_PREFIX.len();
    let record: WireRecord = serde_json::from_str(&line[payload_at..]).ok()?;
    if record.schema != BRIDGE_SCHEMA {
        return None;
    }
    let game = |kind| ModSignal::Game(GameEvent::new(record.sequence, record.client_time_ms, kind));
    match record.kind {
        WireKind::HookReady {} => Some(ModSignal::HookReady),
        WireKind::Kill {} => Some(game(EventKind::Kill)),
        WireKind::Death {} => Some(game(EventKind::Death)),
        WireKind::Assist {} => Some(game(EventKind::Assist)),
        WireKind::Respawn {} => Some(game(EventKind::Respawn)),
        WireKind::AbilityUsed { ability_slot } => {
            Some(game(EventKind::AbilityUsed { slot: ability_slot }))
        }
        WireKind::AbilityReady { ability_slot } => {
            Some(game(EventKind::AbilityReady { slot: ability_slot }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(event: &str) -> String {
        format!(
            "[DEADASS]{{\"schema\":1,\"event\":\"{event}\",\"mod_version\":\"0.1.0\",\"session_id\":\"abc\",\"sequence\":7,\"client_time_ms\":4242}}"
        )
    }

    #[test]
    fn kill_maps_to_game_event() {
        let Some(ModSignal::Game(event)) = parse_bridge_line(&record("kill")) else {
            panic!("kill line must parse");
        };
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
