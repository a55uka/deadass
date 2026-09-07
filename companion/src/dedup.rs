use deadass_shared::{GameEvent, TriggerKind, now_ms};
use std::collections::VecDeque;
use std::time::Duration;

const MAX_TRACKED_TRIGGERS: usize = 64;

pub struct EventDeduplicator {
    window: Duration,
    recent: VecDeque<(TriggerKind, u64)>,
}

impl EventDeduplicator {
    pub fn new(window: Duration) -> Self {
        Self {
            window,
            recent: VecDeque::new(),
        }
    }

    pub fn should_emit(&mut self, event: GameEvent) -> bool {
        self.should_emit_at(TriggerKind::from(event.kind), now_ms())
    }

    fn should_emit_at(&mut self, trigger: TriggerKind, now: u64) -> bool {
        let window_ms = self.window.as_millis() as u64;
        self.recent
            .retain(|(_, seen_at)| now.saturating_sub(*seen_at) <= window_ms);
        if self.recent.iter().any(|(known, _)| *known == trigger) {
            return false;
        }
        self.recent.push_back((trigger, now));
        if self.recent.len() > MAX_TRACKED_TRIGGERS {
            self.recent.pop_front();
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadass_shared::EventKind;

    fn kill() -> GameEvent {
        GameEvent::new(1, 0, EventKind::Kill)
    }

    #[test]
    fn immediate_repeat_is_dropped() {
        let mut dedup = EventDeduplicator::new(Duration::from_millis(200));
        assert!(dedup.should_emit_at(TriggerKind::Kill, 1000));
        assert!(!dedup.should_emit_at(TriggerKind::Kill, 1100));
    }

    #[test]
    fn repeat_after_window_passes() {
        let mut dedup = EventDeduplicator::new(Duration::from_millis(200));
        assert!(dedup.should_emit_at(TriggerKind::Kill, 1000));
        assert!(dedup.should_emit_at(TriggerKind::Kill, 1300));
    }

    #[test]
    fn stale_sender_timestamp_does_not_suppress() {
        let mut dedup = EventDeduplicator::new(Duration::from_millis(200));
        assert!(dedup.should_emit(kill()));
        std::thread::sleep(Duration::from_millis(250));
        assert!(dedup.should_emit(kill()));
    }
}
