use deadass_shared::{AppConfig, GameEvent, Pattern, TriggerKind, now_ms};
use std::collections::HashMap;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HapticCommand {
    pub strength: f64,
    pub duration_ms: u64,
    pub pattern: Pattern,
}

impl HapticCommand {
    pub fn from_trigger(config: &AppConfig, kind: TriggerKind) -> Option<Self> {
        let rule = config.trigger(kind);
        if !rule.enabled {
            return None;
        }
        Some(Self {
            strength: rule.scaled_strength(config.master_gain, config.max_strength_cap),
            duration_ms: rule.duration_ms,
            pattern: rule.pattern,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuppressReason {
    Disabled,
    MutedWhileDead,
    Cooldown,
}

impl fmt::Display for SuppressReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Disabled => write!(f, "disabled in config"),
            Self::MutedWhileDead => write!(f, "muted while dead"),
            Self::Cooldown => write!(f, "retrigger cooldown"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GateDecision {
    Fire(HapticCommand),
    Suppress(SuppressReason),
}

#[derive(Default)]
pub struct HapticGate {
    last_fired_ms: HashMap<TriggerKind, u64>,
    dead_since_ms: Option<u64>,
}

impl HapticGate {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn decide(&mut self, config: &AppConfig, event: GameEvent) -> GateDecision {
        self.track_liveness(event);
        if config.mute_while_dead && self.dead_since_ms.is_some() {
            return GateDecision::Suppress(SuppressReason::MutedWhileDead);
        }
        let trigger = TriggerKind::from(event.kind);
        let Some(command) = HapticCommand::from_trigger(config, trigger) else {
            return GateDecision::Suppress(SuppressReason::Disabled);
        };
        let now = now_ms().max(event.wall_time_ms);
        if self.fired_recently(trigger, now, config.trigger(trigger).retrigger_cooldown_ms) {
            return GateDecision::Suppress(SuppressReason::Cooldown);
        }
        self.last_fired_ms.insert(trigger, now);
        GateDecision::Fire(command)
    }

    fn track_liveness(&mut self, event: GameEvent) {
        use deadass_shared::EventKind;
        match event.kind {
            EventKind::Death => self.dead_since_ms = Some(event.wall_time_ms),
            EventKind::Respawn => self.dead_since_ms = None,
            _ => {}
        }
    }

    fn fired_recently(&self, trigger: TriggerKind, now: u64, cooldown_ms: u64) -> bool {
        self.last_fired_ms
            .get(&trigger)
            .is_some_and(|last| now.saturating_sub(*last) < cooldown_ms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadass_shared::EventKind;

    fn kill_at(wall_time_ms: u64) -> GameEvent {
        GameEvent::new(1, wall_time_ms, EventKind::Kill)
    }

    fn death_at(wall_time_ms: u64) -> GameEvent {
        GameEvent::new(1, wall_time_ms, EventKind::Death)
    }

    fn respawn_at(wall_time_ms: u64) -> GameEvent {
        GameEvent::new(2, wall_time_ms, EventKind::Respawn)
    }

    #[test]
    fn second_immediate_kill_is_debounced() {
        let config = AppConfig::default();
        let mut gate = HapticGate::new();
        assert!(matches!(
            gate.decide(&config, kill_at(1000)),
            GateDecision::Fire(_)
        ));
        assert!(matches!(
            gate.decide(&config, kill_at(1001)),
            GateDecision::Suppress(SuppressReason::Cooldown)
        ));
    }

    #[test]
    fn mute_while_dead_suppresses_until_respawn() {
        let config = AppConfig {
            mute_while_dead: true,
            ..AppConfig::default()
        };
        let mut gate = HapticGate::new();
        gate.decide(&config, death_at(1000));
        assert_eq!(
            gate.decide(&config, kill_at(2000)),
            GateDecision::Suppress(SuppressReason::MutedWhileDead)
        );
        gate.decide(&config, respawn_at(3000));
        assert!(matches!(
            gate.decide(&config, kill_at(4000)),
            GateDecision::Fire(_)
        ));
    }
}
