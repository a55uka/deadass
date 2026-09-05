use deadass_shared::{EventKind, EventSource, GameEvent, now_ms};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScoreboardSnapshot {
    pub kills: i32,
    pub deaths: i32,
    pub assists: i32,
    pub alive: bool,
}

#[derive(Debug, Clone, Default)]
pub struct ScoreboardDelta {
    previous: Option<ScoreboardSnapshot>,
    sequence: u64,
}

impl ScoreboardDelta {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn diff(&mut self, current: ScoreboardSnapshot) -> Vec<GameEvent> {
        let mut emitted = Vec::new();
        let Some(previous) = self.previous else {
            self.previous = Some(current);
            return emitted;
        };
        let now = now_ms();
        if current.kills > previous.kills {
            for _ in previous.kills..current.kills {
                emitted.push(GameEvent::new(
                    next_sequence(&mut self.sequence),
                    now,
                    EventSource::Dll,
                    EventKind::Kill,
                ));
            }
        }
        if current.assists > previous.assists {
            for _ in previous.assists..current.assists {
                emitted.push(GameEvent::new(
                    next_sequence(&mut self.sequence),
                    now,
                    EventSource::Dll,
                    EventKind::Assist,
                ));
            }
        }
        if current.deaths > previous.deaths {
            for _ in previous.deaths..current.deaths {
                emitted.push(GameEvent::new(
                    next_sequence(&mut self.sequence),
                    now,
                    EventSource::Dll,
                    EventKind::Death,
                ));
            }
        }
        if !previous.alive && current.alive {
            emitted.push(GameEvent::new(
                next_sequence(&mut self.sequence),
                now,
                EventSource::Dll,
                EventKind::Respawn,
            ));
        }
        self.previous = Some(current);
        emitted
    }
}

fn next_sequence(sequence: &mut u64) -> u64 {
    *sequence = sequence.wrapping_add(1);
    *sequence
}
