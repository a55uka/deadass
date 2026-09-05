use deadass_shared::{EventKind, EventSource, GameEvent, now_ms};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ExternalSnapshot {
    pub kills: i32,
    pub deaths: i32,
    pub assists: i32,
    pub alive: bool,
    pub ability_cooldown_ready: [bool; 4],
}

pub struct ExternalPoller {
    previous: Option<ExternalSnapshot>,
    sequence: u64,
}

impl ExternalPoller {
    pub fn new() -> Self {
        Self {
            previous: None,
            sequence: 0,
        }
    }

    pub fn observe(&mut self, current: ExternalSnapshot) -> Vec<GameEvent> {
        let Some(previous) = self.previous else {
            self.previous = Some(current);
            return Vec::new();
        };
        let mut emitted = Vec::new();
        let now = now_ms();
        emitted.extend(counter_events(
            &mut self.sequence,
            now,
            previous.kills,
            current.kills,
            EventKind::Kill,
        ));
        emitted.extend(counter_events(
            &mut self.sequence,
            now,
            previous.deaths,
            current.deaths,
            EventKind::Death,
        ));
        emitted.extend(counter_events(
            &mut self.sequence,
            now,
            previous.assists,
            current.assists,
            EventKind::Assist,
        ));
        if !previous.alive && current.alive {
            emitted.push(self.stamped(now, EventKind::Respawn));
        }
        for slot in 0..4 {
            if !previous.ability_cooldown_ready[slot] && current.ability_cooldown_ready[slot] {
                emitted.push(self.stamped(now, EventKind::AbilityReady { slot: slot as u8 }));
            }
        }
        self.previous = Some(current);
        emitted
    }

    fn stamped(&mut self, now: u64, kind: EventKind) -> GameEvent {
        self.sequence = self.sequence.wrapping_add(1);
        GameEvent::new(self.sequence, now, EventSource::External, kind)
    }
}

impl Default for ExternalPoller {
    fn default() -> Self {
        Self::new()
    }
}

fn counter_events(
    sequence: &mut u64,
    now: u64,
    before: i32,
    after: i32,
    kind: EventKind,
) -> Vec<GameEvent> {
    if after <= before {
        return Vec::new();
    }
    (before..after)
        .map(|_| {
            *sequence = sequence.wrapping_add(1);
            GameEvent::new(*sequence, now, EventSource::External, kind)
        })
        .collect()
}

#[cfg(test)]
mod poller_reports_edges {
    use super::*;

    fn snapshot(kills: i32, alive: bool, ready: [bool; 4]) -> ExternalSnapshot {
        ExternalSnapshot {
            kills,
            deaths: 0,
            assists: 0,
            alive,
            ability_cooldown_ready: ready,
        }
    }

    #[test]
    fn first_observation_seeds_without_emitting() {
        let mut poller = ExternalPoller::new();
        assert!(poller.observe(snapshot(0, true, [true; 4])).is_empty());
    }

    #[test]
    fn kill_and_respawn_and_ready_each_emit_once() {
        let mut poller = ExternalPoller::new();
        poller.observe(snapshot(0, false, [false; 4]));
        let emitted = poller.observe(snapshot(1, true, [true, false, false, false]));
        let kinds: Vec<EventKind> = emitted.iter().map(|event| event.kind).collect();
        assert!(kinds.contains(&EventKind::Kill));
        assert!(kinds.contains(&EventKind::Respawn));
        assert!(kinds.contains(&EventKind::AbilityReady { slot: 0 }));
    }
}
