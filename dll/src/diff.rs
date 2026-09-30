use deadass_shared::{EventKind, GameEvent};

const ABILITY_SETTLE_MS: u64 = 2000;

const COOLDOWN_LEAD_EPSILON: f32 = 0.05;
const PARRY_SLOT_MIN: u8 = 4;
/// Sanity bound on m_flParrySuccessEndTime: a live success window ends a few
/// seconds out; anything further is stale garbage, not a landed parry.
const PARRY_SUCCESS_WINDOW_MAX: f32 = 60.0;
const HERO_SLOT_LIMIT: u8 = 4;
const DEFAULT_MELEE_SLOT: u8 = 22;

const SWING_ECHO_MS: u64 = 500;
const INTERRUPT_ECHO_MS: u64 = 500;
const STUN_CONFIRM_MS: u64 = 300;
const ENEMY_HIT_QUIET_MS: u64 = 100;

#[derive(Debug, Clone, PartialEq)]
pub struct PawnSnapshot {
    pub address: u64,
    pub health: i32,
    pub life_state: u8,
    pub game_time: f32,
    pub interrupted: bool,
    pub melee_threat: bool,
    pub abilities: Vec<AbilitySnapshot>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayerSnapshot {
    pub address: u64,
    pub is_local: bool,
    pub hero_id: u32,
    pub kills: i32,
    pub assists: i32,
    pub deaths: i32,
    pub kill_streak: i32,
    pub alive: bool,
    pub health: i32,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Snapshot {
    pub pawn: Option<PawnSnapshot>,
    pub players: Vec<PlayerSnapshot>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AbilitySnapshot {
    pub slot: u8,
    pub charges: Option<i32>,
    pub cooldown_end: f32,
    pub channeling: bool,
    pub melee_state: u32,
    pub parry_success_end: f32,
}

impl AbilitySnapshot {
    fn melee_active(&self) -> bool {
        self.melee_state != 0 || self.channeling
    }

    fn parry_success_active(&self, game_time: f32) -> bool {
        self.parry_success_end > game_time
            && self.parry_success_end < game_time + PARRY_SUCCESS_WINDOW_MAX
    }
}

#[derive(Debug, Clone, Copy)]
struct TrackedAbility {
    charges: Option<i32>,
    cooling: bool,
    channeling: bool,
    melee_state: u32,
    parry_success_end: f32,
}

fn track(snapshot: AbilitySnapshot, game_time: f32) -> TrackedAbility {
    TrackedAbility {
        charges: snapshot.charges,
        cooling: snapshot.cooldown_end > game_time + COOLDOWN_LEAD_EPSILON,
        channeling: snapshot.channeling,
        melee_state: snapshot.melee_state,
        parry_success_end: snapshot.parry_success_end,
    }
}

impl TrackedAbility {
    fn melee_active(&self) -> bool {
        self.melee_state != 0 || self.channeling
    }

    fn parry_success_active(&self, game_time: f32) -> bool {
        self.parry_success_end > game_time
            && self.parry_success_end < game_time + PARRY_SUCCESS_WINDOW_MAX
    }
}

#[derive(Debug, Default)]
pub struct Monitor {
    previous: Option<TrackedPawn>,
    players: std::collections::BTreeMap<u64, TrackedPlayer>,
    sequence: u64,
    ability_settle_until_ms: u64,
    melee_slot: u8,
    melee_swing_until_ms: u64,
    last_interrupt_ms: u64,
    interrupt_since_ms: u64,
    interrupt_health: i32,
    interrupt_swinging: bool,
    interrupt_enemies_quiet: bool,
    parried_emitted: bool,
    enemy_health_drop_ms: u64,
}

#[derive(Debug)]
struct TrackedPawn {
    address: u64,
    alive: bool,
    health: i32,
    interrupted: bool,
    abilities: std::collections::BTreeMap<u8, TrackedAbility>,
}

#[derive(Debug, Clone, Copy)]
struct TrackedPlayer {
    kills: i32,
    assists: i32,
    health: i32,
}

impl Monitor {
    pub fn new() -> Self {
        Self {
            melee_slot: DEFAULT_MELEE_SLOT,
            ..Self::default()
        }
    }

    pub fn update(&mut self, snapshot: Snapshot, now_ms: u64) -> Vec<GameEvent> {
        let mut events = self.update_players(&snapshot.players, snapshot.pawn.as_ref(), now_ms);
        events.extend(self.update_pawn(snapshot.pawn, now_ms));
        events
    }

    pub fn set_melee_slot(&mut self, slot: u8) {
        self.melee_slot = slot;
    }

    fn reset_interrupt_tracking(&mut self) {
        self.melee_swing_until_ms = 0;
        self.last_interrupt_ms = 0;
        self.interrupt_since_ms = 0;
        self.interrupt_health = 0;
        self.interrupt_swinging = false;
        self.interrupt_enemies_quiet = true;
        self.parried_emitted = false;
        self.enemy_health_drop_ms = 0;
    }

    fn update_pawn(&mut self, snapshot: Option<PawnSnapshot>, now_ms: u64) -> Vec<GameEvent> {
        let Some(snapshot) = snapshot else {
            self.previous = None;
            self.players.clear();
            self.reset_interrupt_tracking();
            return Vec::new();
        };

        let alive = snapshot.life_state == 0;
        let mut abilities = std::collections::BTreeMap::new();
        for ability in &snapshot.abilities {
            abilities.insert(ability.slot, track(*ability, snapshot.game_time));
        }
        if snapshot
            .abilities
            .iter()
            .any(|ability| ability.slot == self.melee_slot && ability.melee_active())
        {
            self.melee_swing_until_ms = now_ms + SWING_ECHO_MS;
        }

        let Some(previous) = self.previous.take() else {
            self.reset_interrupt_tracking();
            self.previous = Some(TrackedPawn {
                address: snapshot.address,
                alive,
                health: snapshot.health,
                interrupted: snapshot.interrupted,
                abilities,
            });
            return Vec::new();
        };

        let mut events = Vec::new();
        if previous.address != snapshot.address {
            if !previous.alive && alive {
                events.push(self.emit(EventKind::Respawn, now_ms));
            }
            self.ability_settle_until_ms = now_ms + ABILITY_SETTLE_MS;
            self.reset_interrupt_tracking();
            self.previous = Some(TrackedPawn {
                address: snapshot.address,
                alive,
                health: snapshot.health,
                interrupted: snapshot.interrupted,
                abilities,
            });
            return events;
        }

        if previous.alive && !alive {
            events.push(self.emit(EventKind::Death, now_ms));
            self.ability_settle_until_ms = now_ms + ABILITY_SETTLE_MS;
            self.reset_interrupt_tracking();
        } else if !previous.alive && alive {
            events.push(self.emit(EventKind::Respawn, now_ms));
            self.ability_settle_until_ms = now_ms + ABILITY_SETTLE_MS;
            self.reset_interrupt_tracking();
        }

        let swinging_now = snapshot
            .abilities
            .iter()
            .any(|ability| ability.slot == self.melee_slot && ability.melee_active());
        let swinging_before = previous
            .abilities
            .get(&self.melee_slot)
            .is_some_and(TrackedAbility::melee_active);
        if snapshot.interrupted && !previous.interrupted {
            self.interrupt_since_ms = now_ms;
            self.interrupt_health = previous.health;
            self.interrupt_swinging = swinging_now || swinging_before;
            self.interrupt_enemies_quiet =
                now_ms.saturating_sub(self.enemy_health_drop_ms) >= ENEMY_HIT_QUIET_MS;
            self.parried_emitted = false;
            self.last_interrupt_ms = now_ms;
        } else if snapshot.interrupted && self.interrupt_since_ms == 0 {
            self.interrupt_since_ms = now_ms;
            self.interrupt_health = previous.health;
            self.interrupt_swinging = false;
            self.parried_emitted = false;
            self.last_interrupt_ms = now_ms;
        } else if !snapshot.interrupted {
            self.interrupt_since_ms = 0;
            self.parried_emitted = false;
        }

        let settled = alive && now_ms >= self.ability_settle_until_ms;
        let mut parry_emitted = false;
        for (slot, current) in &abilities {
            let Some(tracked) = previous.abilities.get(slot) else {
                continue;
            };
            let transitions = ability_transitions(*slot, tracked, current, snapshot.game_time);
            if transitions.used && settled && *slot < HERO_SLOT_LIMIT {
                events.push(self.emit(EventKind::AbilityUsed { slot: *slot }, now_ms));
            }
            if transitions.ready && settled && *slot < HERO_SLOT_LIMIT {
                events.push(self.emit(EventKind::AbilityReady { slot: *slot }, now_ms));
            }
            if transitions.parry_caught
                && settled
                && !parry_emitted
                && now_ms >= self.melee_swing_until_ms
                && now_ms.saturating_sub(self.last_interrupt_ms) >= INTERRUPT_ECHO_MS
            {
                events.push(self.emit(EventKind::Parry, now_ms));
                parry_emitted = true;
            }
        }

        let stunned_long = snapshot.interrupted
            && self.interrupt_since_ms != 0
            && now_ms.saturating_sub(self.interrupt_since_ms) >= STUN_CONFIRM_MS;
        if settled
            && stunned_long
            && !self.parried_emitted
            && self.interrupt_swinging
            && snapshot.health >= self.interrupt_health
            && self.interrupt_enemies_quiet
        {
            events.push(self.emit(EventKind::Parried, now_ms));
            self.parried_emitted = true;
        }

        // Punch taken: the reader attributed this tick's damage to an enemy
        // melee swing (see game::reader's lazy sweep).
        if settled && snapshot.melee_threat {
            events.push(self.emit(EventKind::PunchTaken, now_ms));
        }

        self.previous = Some(TrackedPawn {
            address: snapshot.address,
            alive,
            health: snapshot.health,
            interrupted: snapshot.interrupted,
            abilities,
        });
        events
    }

    fn update_players(
        &mut self,
        players: &[PlayerSnapshot],
        pawn: Option<&PawnSnapshot>,
        now_ms: u64,
    ) -> Vec<GameEvent> {
        let mut events = Vec::new();
        if players.is_empty() {
            return events;
        }

        let our_swing_context = pawn.is_some_and(|pawn| {
            pawn.abilities.iter().any(|ability| {
                (ability.slot == self.melee_slot && ability.melee_active())
                    || ability.parry_success_active(pawn.game_time)
            })
        });
        let settled = now_ms >= self.ability_settle_until_ms;
        let mut punch_emitted = false;

        for player in players {
            let Some(&previous) = self.players.get(&player.address) else {
                self.players.insert(
                    player.address,
                    TrackedPlayer {
                        kills: player.kills,
                        assists: player.assists,
                        health: player.health,
                    },
                );
                continue;
            };

            let clamp_delta = |was: i32, now: i32| -> u32 { now.saturating_sub(was).max(0) as u32 };

            if player.is_local {
                for _ in 0..clamp_delta(previous.kills, player.kills) {
                    events.push(self.emit(EventKind::Kill, now_ms));
                }
                for _ in 0..clamp_delta(previous.assists, player.assists) {
                    events.push(self.emit(EventKind::Assist, now_ms));
                }
            } else if previous.health - player.health >= 1 {
                self.enemy_health_drop_ms = now_ms;
                if !punch_emitted && settled && our_swing_context {
                    events.push(self.emit(EventKind::PunchLanded, now_ms));
                    punch_emitted = true;
                }
            }

            self.players.insert(
                player.address,
                TrackedPlayer {
                    kills: player.kills,
                    assists: player.assists,
                    health: player.health,
                },
            );
        }
        events
    }

    fn emit(&mut self, kind: EventKind, now_ms: u64) -> GameEvent {
        self.sequence = self.sequence.wrapping_add(1);
        GameEvent::new(self.sequence, now_ms, kind)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Transitions {
    used: bool,
    ready: bool,
    parry_caught: bool,
}

fn ability_transitions(
    slot: u8,
    previous: &TrackedAbility,
    current: &TrackedAbility,
    game_time: f32,
) -> Transitions {
    let charge_used = previous
        .charges
        .zip(current.charges)
        .is_some_and(|(was, now)| now < was);
    let charge_ready = previous
        .charges
        .zip(current.charges)
        .is_some_and(|(was, now)| now > was);
    let channel_started = !previous.channeling && current.channeling;

    let parry_caught =
        current.parry_success_active(game_time) && !previous.parry_success_active(game_time);

    Transitions {
        used: charge_used
            || (!previous.cooling && current.cooling)
            || (channel_started && slot < PARRY_SLOT_MIN),
        ready: charge_ready || (previous.cooling && !current.cooling),
        parry_caught,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pawn(alive: bool) -> PawnSnapshot {
        PawnSnapshot {
            address: 0x1000,
            health: if alive { 600 } else { 0 },
            life_state: if alive { 0 } else { 1 },
            game_time: 100.0,
            interrupted: false,
            melee_threat: false,
            abilities: Vec::new(),
        }
    }

    fn ability(slot: u8, charges: Option<i32>, cooldown_end: f32) -> AbilitySnapshot {
        AbilitySnapshot {
            slot,
            charges,
            cooldown_end,
            channeling: false,
            melee_state: 0,
            parry_success_end: 0.0,
        }
    }

    fn swinging(slot: u8, state: u32) -> AbilitySnapshot {
        let mut ability = ability(slot, None, 0.0);
        ability.melee_state = state;
        ability
    }

    fn parry_success(slot: u8, game_time: f32) -> AbilitySnapshot {
        let mut ability = ability(slot, None, 0.0);
        ability.parry_success_end = game_time + 3.0;
        ability
    }

    fn player(address: u64, is_local: bool, kills: i32, assists: i32) -> PlayerSnapshot {
        PlayerSnapshot {
            address,
            is_local,
            hero_id: 1,
            kills,
            assists,
            deaths: 0,
            kill_streak: 0,
            alive: true,
            health: 500,
        }
    }

    fn track(slots: Vec<AbilitySnapshot>, game_time: f32) -> Snapshot {
        let mut snapshot = pawn(true);
        snapshot.game_time = game_time;
        snapshot.abilities = slots;
        Snapshot {
            pawn: Some(snapshot),
            players: Vec::new(),
        }
    }

    fn bare(pawn: PawnSnapshot) -> Snapshot {
        Snapshot {
            pawn: Some(pawn),
            players: Vec::new(),
        }
    }

    fn with_players(pawn: PawnSnapshot, players: Vec<PlayerSnapshot>) -> Snapshot {
        Snapshot {
            pawn: Some(pawn),
            players,
        }
    }

    fn kinds(events: &[GameEvent]) -> Vec<EventKind> {
        events.iter().map(|event| event.kind).collect()
    }

    #[test]
    fn first_snapshot_baselines_without_events() {
        let mut monitor = Monitor::new();
        assert!(
            monitor
                .update(track(vec![ability(0, None, 0.0)], 100.0), 1000)
                .is_empty()
        );
    }

    #[test]
    fn death_and_respawn_emit_in_order() {
        let mut monitor = Monitor::new();
        monitor.update(track(vec![], 100.0), 1000);
        let death = monitor.update(bare(pawn(false)), 1100);
        assert_eq!(kinds(&death), vec![EventKind::Death]);
        let respawn = monitor.update(bare(pawn(true)), 3000);
        assert_eq!(kinds(&respawn), vec![EventKind::Respawn]);
    }

    #[test]
    fn menu_clears_state_so_returning_player_does_not_emit_death() {
        let mut monitor = Monitor::new();
        monitor.update(track(vec![], 100.0), 1000);
        monitor.update(Snapshot::default(), 2000);
        assert!(monitor.update(track(vec![], 100.0), 3000).is_empty());
    }

    #[test]
    fn fresh_pawn_while_dead_counts_as_respawn() {
        let mut monitor = Monitor::new();
        monitor.update(track(vec![], 100.0), 1000);
        monitor.update(bare(pawn(false)), 1100);
        let mut fresh = pawn(true);
        fresh.address = 0x2000;
        assert_eq!(
            kinds(&monitor.update(bare(fresh), 5000)),
            vec![EventKind::Respawn]
        );
    }

    #[test]
    fn every_cooldown_emits_used_then_ready() {
        let mut monitor = Monitor::new();
        monitor.update(track(vec![ability(2, None, 0.0)], 100.0), 1000);

        let used = monitor.update(track(vec![ability(2, None, 125.0)], 100.0), 1033);
        assert_eq!(kinds(&used), vec![EventKind::AbilityUsed { slot: 2 }]);

        let ready = monitor.update(track(vec![ability(2, None, 125.0)], 125.5), 9000);
        assert_eq!(kinds(&ready), vec![EventKind::AbilityReady { slot: 2 }]);

        let used = monitor.update(track(vec![ability(2, None, 160.0)], 130.0), 9500);
        assert_eq!(kinds(&used), vec![EventKind::AbilityUsed { slot: 2 }]);
        let ready = monitor.update(track(vec![ability(2, None, 160.0)], 160.5), 12000);
        assert_eq!(kinds(&ready), vec![EventKind::AbilityReady { slot: 2 }]);
    }

    #[test]
    fn stale_cooldown_end_value_does_not_retrigger_ready() {
        let mut monitor = Monitor::new();
        monitor.update(track(vec![ability(2, None, 0.0)], 100.0), 1000);
        monitor.update(track(vec![ability(2, None, 125.0)], 100.0), 1033);
        monitor.update(track(vec![ability(2, None, 125.0)], 125.5), 9000);
        let events = monitor.update(track(vec![ability(2, None, 125.0)], 200.0), 9300);
        assert!(events.is_empty());
    }

    #[test]
    fn charge_decrement_and_restore_emit_events() {
        let mut monitor = Monitor::new();
        monitor.update(track(vec![ability(1, Some(3), 0.0)], 100.0), 1000);
        let used = monitor.update(track(vec![ability(1, Some(2), 0.0)], 100.0), 1033);
        assert_eq!(kinds(&used), vec![EventKind::AbilityUsed { slot: 1 }]);
        let ready = monitor.update(track(vec![ability(1, Some(3), 0.0)], 100.0), 10_000);
        assert_eq!(kinds(&ready), vec![EventKind::AbilityReady { slot: 1 }]);
    }

    #[test]
    fn abilities_right_after_respawn_are_rebaselined_not_emitted() {
        let mut monitor = Monitor::new();
        monitor.update(track(vec![ability(0, Some(1), 0.0)], 100.0), 1000);
        monitor.update(bare(pawn(false)), 1100);
        monitor.update(bare(pawn(true)), 3000);
        let events = monitor.update(track(vec![ability(0, Some(3), 0.0)], 100.0), 3200);
        assert!(events.is_empty());
        let used = monitor.update(track(vec![ability(0, Some(2), 0.0)], 100.0), 6000);
        assert_eq!(kinds(&used), vec![EventKind::AbilityUsed { slot: 0 }]);
    }

    #[test]
    fn sequences_increase_monotonically() {
        let mut monitor = Monitor::new();
        monitor.update(track(vec![], 100.0), 1000);
        let first = monitor.update(bare(pawn(false)), 1100);
        let second = monitor.update(bare(pawn(true)), 4000);
        assert!(second[0].sequence > first[0].sequence);
    }

    #[test]
    fn parry_success_window_edge_emits_parry() {
        let mut monitor = Monitor::new();
        monitor.update(track(vec![ability(22, None, 0.0)], 100.0), 1000);

        let events = monitor.update(track(vec![parry_success(22, 100.0)], 100.5), 1033);
        assert_eq!(kinds(&events), vec![EventKind::Parry]);

        // The window is still open on later ticks — no re-emit.
        let events = monitor.update(track(vec![parry_success(22, 100.0)], 101.0), 1100);
        assert!(events.is_empty());
    }

    #[test]
    fn stale_or_garbage_parry_success_timestamps_do_not_emit() {
        let mut monitor = Monitor::new();
        monitor.update(track(vec![ability(22, None, 0.0)], 100.0), 1000);

        let events = monitor.update(track(vec![ability(22, None, 0.0)], 100.5), 1033);
        assert!(events.is_empty());

        let mut garbage = ability(22, None, 0.0);
        garbage.parry_success_end = 100.0 + 10_000.0;
        let events = monitor.update(track(vec![garbage], 100.5), 1033);
        assert!(events.is_empty());
    }

    #[test]
    fn parry_field_that_rises_from_our_own_swing_is_not_a_parry() {
        let mut monitor = Monitor::new();
        monitor.update(track(vec![swinging(22, 3)], 100.0), 1000);

        let mut rearmed = swinging(22, 3);
        rearmed.parry_success_end = 103.0;
        let events = monitor.update(track(vec![rearmed], 100.5), 1033);
        assert!(events.is_empty());

        let mut echoed = ability(22, None, 0.0);
        echoed.parry_success_end = 103.0;
        let events = monitor.update(track(vec![echoed], 101.0), 1200);
        assert!(events.is_empty());
    }

    #[test]
    fn parry_field_rising_right_after_an_interrupt_is_hit_confirm_noise() {
        // Punching someone: the interrupt flips on hit-confirm and the
        // parry field rearms a tick later — neither is a parry.
        let mut monitor = Monitor::new();
        monitor.update(
            with_players(
                pawn(true),
                vec![player(0x10, true, 0, 0), player(0x20, false, 0, 0)],
            ),
            1000,
        );

        let mut striking = pawn(true);
        striking.abilities = vec![swinging(22, 3)];
        let mut hit = player(0x20, false, 0, 0);
        hit.health = 420;
        let events = monitor.update(
            with_players(striking, vec![player(0x10, true, 0, 0), hit]),
            1033,
        );
        assert_eq!(kinds(&events), vec![EventKind::PunchLanded]);

        let mut confirm = pawn(true);
        confirm.interrupted = true;
        confirm.abilities = vec![parry_success(22, 100.0)];
        let events = monitor.update(
            with_players(confirm, vec![player(0x10, true, 0, 0), hit]),
            1066,
        );
        assert!(events.is_empty());
    }

    #[test]
    fn interrupt_during_our_swing_emits_parried_once_the_stun_holds() {
        let mut monitor = Monitor::new();
        monitor.update(track(vec![swinging(22, 3)], 100.0), 1000);

        let mut countered = pawn(true);
        countered.interrupted = true;
        let events = monitor.update(bare(countered), 1033);
        assert!(events.is_empty());

        let mut held = pawn(true);
        held.interrupted = true;
        let events = monitor.update(bare(held), 1367);
        assert_eq!(kinds(&events), vec![EventKind::Parried]);

        let mut held = pawn(true);
        held.interrupted = true;
        let events = monitor.update(bare(held), 1400);
        assert!(events.is_empty());
    }

    #[test]
    fn brief_hit_stop_from_our_own_landed_hit_is_not_parried() {
        let mut monitor = Monitor::new();
        monitor.update(
            with_players(
                pawn(true),
                vec![player(0x10, true, 0, 0), player(0x20, false, 0, 0)],
            ),
            1000,
        );

        let mut confirm = pawn(true);
        confirm.interrupted = true;
        let mut hit = player(0x20, false, 0, 0);
        hit.health = 420;
        monitor.update(
            with_players(confirm, vec![player(0x10, true, 0, 0), hit]),
            1033,
        );

        let mut cleared = pawn(true);
        cleared.abilities = vec![swinging(22, 0)];
        let events = monitor.update(
            with_players(cleared, vec![player(0x10, true, 0, 0), hit]),
            1166,
        );
        assert!(events.is_empty());
    }

    #[test]
    fn interrupt_without_our_melee_in_motion_is_not_parried() {
        let mut monitor = Monitor::new();
        monitor.update(track(vec![ability(0, None, 0.0)], 100.0), 1000);
        let mut stunned = pawn(true);
        stunned.interrupted = true;
        monitor.update(bare(stunned), 1033);
        let mut held = pawn(true);
        held.interrupted = true;
        let events = monitor.update(bare(held), 1700);
        assert!(events.is_empty());
    }

    #[test]
    fn interrupt_with_damage_taken_is_a_trade_not_a_parry() {
        let mut monitor = Monitor::new();
        monitor.update(track(vec![swinging(22, 3)], 100.0), 1000);

        let mut traded = pawn(true);
        traded.health = 380;
        traded.interrupted = true;
        monitor.update(bare(traded), 1033);
        let mut held = pawn(true);
        held.health = 380;
        held.interrupted = true;
        let events = monitor.update(bare(held), 1700);
        assert!(events.is_empty());
    }

    #[test]
    fn enemy_health_drop_during_our_swing_emits_punch_landed() {
        let mut monitor = Monitor::new();
        monitor.update(
            with_players(
                pawn(true),
                vec![player(0x10, true, 0, 0), player(0x20, false, 0, 0)],
            ),
            1000,
        );

        let mut striking = pawn(true);
        striking.abilities = vec![swinging(22, 2)];
        let mut hit = player(0x20, false, 0, 0);
        hit.health = 420;
        let events = monitor.update(
            with_players(striking, vec![player(0x10, true, 0, 0), hit]),
            1033,
        );
        assert_eq!(kinds(&events), vec![EventKind::PunchLanded]);

        let mut striking = pawn(true);
        striking.abilities = vec![swinging(22, 2)];
        let events = monitor.update(
            with_players(striking, vec![player(0x10, true, 0, 0), hit]),
            1066,
        );
        assert!(events.is_empty());
    }

    #[test]
    fn punch_landed_still_fires_when_the_state_cleared_before_the_damage_showed() {
        let mut monitor = Monitor::new();
        let mut recent = pawn(true);
        recent.abilities = vec![parry_success(22, 100.0)];
        monitor.update(
            with_players(recent, vec![player(0x10, true, 0, 0), player(0x20, false, 0, 0)]),
            1000,
        );

        let mut cleared = pawn(true);
        cleared.game_time = 101.0;
        cleared.abilities = vec![parry_success(22, 100.0)];
        let mut hit = player(0x20, false, 0, 0);
        hit.health = 420;
        let events = monitor.update(
            with_players(cleared, vec![player(0x10, true, 0, 0), hit]),
            1033,
        );
        assert_eq!(kinds(&events), vec![EventKind::PunchLanded]);
    }

    #[test]
    fn enemy_health_drop_without_our_swing_is_not_punch_landed() {
        let mut monitor = Monitor::new();
        monitor.update(
            with_players(
                pawn(true),
                vec![player(0x10, true, 0, 0), player(0x20, false, 0, 0)],
            ),
            1000,
        );

        let mut hit = player(0x20, false, 0, 0);
        hit.health = 420;
        let events = monitor.update(
            with_players(pawn(true), vec![player(0x10, true, 0, 0), hit]),
            1033,
        );
        assert!(events.is_empty());
    }

    #[test]
    fn melee_threat_on_damage_emits_punch_taken() {
        let mut monitor = Monitor::new();
        monitor.update(
            with_players(
                pawn(true),
                vec![player(0x10, true, 0, 0), player(0x20, false, 0, 0)],
            ),
            1000,
        );

        let mut punched = pawn(true);
        punched.health = 380;
        punched.melee_threat = true;
        let events = monitor.update(
            with_players(
                punched,
                vec![player(0x10, true, 0, 0), player(0x20, false, 0, 0)],
            ),
            1033,
        );
        assert_eq!(kinds(&events), vec![EventKind::PunchTaken]);

        // Bullet damage — the reader found no enemy melee in motion.
        let mut shot = pawn(true);
        shot.health = 300;
        let events = monitor.update(
            with_players(
                shot,
                vec![player(0x10, true, 0, 0), player(0x20, false, 0, 0)],
            ),
            5000,
        );
        assert!(events.is_empty());
    }

    #[test]
    fn punch_taken_is_suppressed_during_the_respawn_settle_window() {
        let mut monitor = Monitor::new();
        monitor.update(track(vec![], 100.0), 1000);
        monitor.update(bare(pawn(false)), 1100);
        monitor.update(bare(pawn(true)), 3000);

        let mut punched = pawn(true);
        punched.melee_threat = true;
        let events = monitor.update(bare(punched), 3200);
        assert!(events.is_empty());
    }

    #[test]
    fn local_player_kill_and_assist_increments_emit_events() {
        let mut monitor = Monitor::new();
        monitor.update(
            with_players(
                pawn(true),
                vec![player(0x10, true, 0, 0), player(0x20, false, 0, 0)],
            ),
            1000,
        );

        let events = monitor.update(
            with_players(
                pawn(true),
                vec![player(0x10, true, 2, 1), player(0x20, false, 0, 0)],
            ),
            1100,
        );
        assert_eq!(
            kinds(&events),
            vec![EventKind::Kill, EventKind::Kill, EventKind::Assist]
        );
    }

    #[test]
    fn other_players_kills_do_not_emit() {
        let mut monitor = Monitor::new();
        monitor.update(
            with_players(
                pawn(true),
                vec![player(0x10, true, 0, 0), player(0x20, false, 0, 0)],
            ),
            1000,
        );
        let events = monitor.update(
            with_players(
                pawn(true),
                vec![player(0x10, true, 0, 0), player(0x20, false, 3, 0)],
            ),
            1100,
        );
        assert!(events.is_empty());
    }

    #[test]
    fn new_players_baseline_without_events() {
        let mut monitor = Monitor::new();
        let events = monitor.update(
            with_players(pawn(true), vec![player(0x10, true, 9, 7)]),
            1000,
        );
        assert!(events.is_empty());
    }

    #[test]
    fn hero_key_channel_start_does_not_emit_parry() {
        let mut monitor = Monitor::new();
        monitor.update(track(vec![ability(2, None, 0.0)], 100.0), 1000);
        let mut channeling = ability(2, None, 0.0);
        channeling.channeling = true;
        let events = monitor.update(track(vec![channeling], 100.0), 1033);
        assert_eq!(kinds(&events), vec![EventKind::AbilityUsed { slot: 2 }]);
    }
}
