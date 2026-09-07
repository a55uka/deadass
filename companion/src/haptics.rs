use deadass_shared::{AppConfig, GameEvent, Pattern, TriggerKind, now_ms};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HapticCommand {
    pub strength: f64,
    pub duration_ms: u64,
    pub pattern: Pattern,
}

/// Why an event did not produce a vibration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuppressReason {
    Disabled,
    MutedWhileDead,
    Cooldown,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GateDecision {
    Fire(HapticCommand),
    Suppress(SuppressReason),
}

#[derive(Default)]
pub struct HapticGate {
    last_fired: HashMap<TriggerKind, u64>,
    dead_since_ms: Option<u64>,
}

impl HapticGate {
    pub fn new() -> Self {
        Self {
            last_fired: HashMap::new(),
            dead_since_ms: None,
        }
    }

    pub fn track(&mut self, event: GameEvent) {
        use deadass_shared::EventKind;
        match event.kind {
            EventKind::Death => self.dead_since_ms = Some(event.wall_time_ms),
            EventKind::Respawn => self.dead_since_ms = None,
            _ => {}
        }
    }
}

pub fn resolve_haptic(
    config: &AppConfig,
    gate: &mut HapticGate,
    event: GameEvent,
) -> Option<HapticCommand> {
    match decide_haptic(config, gate, event) {
        GateDecision::Fire(command) => Some(command),
        GateDecision::Suppress(_) => None,
    }
}

pub fn decide_haptic(
    config: &AppConfig,
    gate: &mut HapticGate,
    event: GameEvent,
) -> GateDecision {
    gate.track(event);
    if config.mute_while_dead && gate.dead_since_ms.is_some() {
        return GateDecision::Suppress(SuppressReason::MutedWhileDead);
    }
    let trigger = TriggerKind::from_event(event.kind);
    let rule = config.trigger(trigger);
    if !rule.enabled {
        return GateDecision::Suppress(SuppressReason::Disabled);
    }
    let now = now_ms().max(event.wall_time_ms);
    if let Some(last) = gate.last_fired.get(&trigger)
        && now.saturating_sub(*last) < rule.retrigger_cooldown_ms
    {
        return GateDecision::Suppress(SuppressReason::Cooldown);
    }
    gate.last_fired.insert(trigger, now);
    GateDecision::Fire(HapticCommand {
        strength: rule.scaled_strength(config.master_gain, config.max_strength_cap),
        duration_ms: rule.duration_ms,
        pattern: rule.pattern,
    })
}

#[cfg(test)]
mod gate_enforces_cooldown_and_mute {
    use super::*;
    use deadass_shared::EventKind;

    fn kill_at(wall_time_ms: u64) -> GameEvent {
        GameEvent::new(1, wall_time_ms, EventKind::Kill)
    }

    #[test]
    fn second_immediate_kill_is_debounced() {
        let config = AppConfig::default();
        let mut gate = HapticGate::new();
        assert!(resolve_haptic(&config, &mut gate, kill_at(1000)).is_some());
        assert!(resolve_haptic(&config, &mut gate, kill_at(1001)).is_none());
    }

    #[test]
    fn mute_while_dead_suppresses_until_respawn() {
        let config = AppConfig {
            mute_while_dead: true,
            ..AppConfig::default()
        };
        let mut gate = HapticGate::new();
        gate.track(GameEvent::new(1, 1000, EventKind::Death));
        assert!(resolve_haptic(&config, &mut gate, kill_at(2000)).is_none());
        gate.track(GameEvent::new(2, 3000, EventKind::Respawn));
        assert!(resolve_haptic(&config, &mut gate, kill_at(4000)).is_some());
    }

    #[test]
    fn decisions_report_suppress_reasons() {
        let config = AppConfig::default();
        let mut gate = HapticGate::new();
        assert!(matches!(
            decide_haptic(&config, &mut gate, kill_at(1000)),
            GateDecision::Fire(_)
        ));
        assert_eq!(
            decide_haptic(&config, &mut gate, kill_at(1001)),
            GateDecision::Suppress(SuppressReason::Cooldown)
        );

        let muted = AppConfig {
            mute_while_dead: true,
            ..AppConfig::default()
        };
        let mut gate = HapticGate::new();
        gate.track(GameEvent::new(1, 1000, EventKind::Death));
        assert_eq!(
            decide_haptic(&muted, &mut gate, kill_at(2000)),
            GateDecision::Suppress(SuppressReason::MutedWhileDead)
        );
    }
}
