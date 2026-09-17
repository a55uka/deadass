use deadass_shared::{EventKind, GameEvent};

const ABILITY_SETTLE_MS: u64 = 2000;

/// m_flSimulationTime — same GameTime_t base
const COOLDOWN_LEAD_EPSILON: f32 = 0.05;

const PARRY_SLOT_MIN: u8 = 4;

const MELEE_ACTIVITY_WINDOW_MS: u64 = 2000;

const PARRY_SUCCESS_FRESH_SECONDS: f32 = 5.0;

const HERO_SLOT_LIMIT: u8 = 4;

const DEFAULT_MELEE_SLOT: u8 = 22;

#[derive(Debug, Clone, PartialEq)]
pub struct PawnSnapshot {
    pub address: u64,
    pub health: i32,
    /// Source 2 LifeState: 0 == alive.
    pub life_state: u8,
    /// (m_flSimulationTime, GameTime_t).
    pub game_time: f32,
    /// CCitadelAbilityComponent.m_bInInterruptState
    pub interrupted: bool,
    /// m_sPlayerDamageTaken.m_flLastDamageTime — game time when a PLAYER
    /// last damaged us.
    pub damage_taken_time: f32,
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
    pub melee_chain: u32,
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
    /// CCitadel_Ability_MeleeParry.m_bAttackParried — only the parry ability
    /// ever sets this; a false->true edge means the parry landed.
    pub attack_parried: bool,
    /// m_flParryStartTime — when the parry window opened (parry class only).
    pub parry_start: f32,
    /// m_flParrySuccessEndTime — used to confirm a fresh parry success.
    pub parry_success_end: f32,
    /// CCitadel_Ability_HoldMelee.m_eCurrentAttackState
    pub melee_state: u32,
    /// CCitadel_Ability_HoldMelee.m_nLightChainCount — increments on landed
    /// light melee hits.
    pub melee_chain: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct TrackedAbility {
    charges: Option<i32>,
    cooling: bool,
    channeling: bool,
    attack_parried: bool,
    parry_start: f32,
    parry_success_end: f32,
    melee_chain: u32,
}

fn track(snapshot: AbilitySnapshot, game_time: f32) -> TrackedAbility {
    TrackedAbility {
        charges: snapshot.charges,
        cooling: snapshot.cooldown_end > game_time + COOLDOWN_LEAD_EPSILON,
        channeling: snapshot.channeling,
        attack_parried: snapshot.attack_parried,
        parry_start: snapshot.parry_start,
        parry_success_end: snapshot.parry_success_end,
        melee_chain: snapshot.melee_chain,
    }
}

#[derive(Debug, Default)]
pub struct Monitor {
    previous: Option<TrackedPawn>,
    players: std::collections::BTreeMap<u64, TrackedPlayer>,
    sequence: u64,
    ability_settle_until_ms: u64,
    melee_activity_until_ms: u64,
    enemy_punch_until_ms: u64,
    /// The weapon-melee ability slot (ESlot_Weapon_Melee = 22).
    melee_slot: u8,
}

#[derive(Debug)]
struct TrackedPawn {
    address: u64,
    alive: bool,
    interrupted: bool,
    damage_taken_time: f32,
    abilities: std::collections::BTreeMap<u8, TrackedAbility>,
}

#[derive(Debug, Clone, Copy)]
struct TrackedPlayer {
    kills: i32,
    assists: i32,
    health: i32,
    melee_chain: u32,
}

impl Monitor {
    pub fn new() -> Self {
        Self {
            melee_slot: DEFAULT_MELEE_SLOT,
            ..Self::default()
        }
    }

    pub fn update(&mut self, snapshot: Snapshot, now_ms: u64) -> Vec<GameEvent> {
        let mut events = self.update_players(&snapshot.players, now_ms);
        events.extend(self.update_pawn(snapshot.pawn, now_ms));
        events
    }

    pub fn set_melee_slot(&mut self, slot: u8) {
        self.melee_slot = slot;
    }

    fn update_pawn(&mut self, snapshot: Option<PawnSnapshot>, now_ms: u64) -> Vec<GameEvent> {
        let Some(snapshot) = snapshot else {
            self.previous = None;
            self.players.clear();
            return Vec::new();
        };

        let alive = snapshot.life_state == 0;
        let mut abilities = std::collections::BTreeMap::new();
        for ability in &snapshot.abilities {
            abilities.insert(ability.slot, track(*ability, snapshot.game_time));
        }

        let melee_active = snapshot.abilities.iter().any(|ability| {
            ability.slot == self.melee_slot
                && (ability.channeling || ability.melee_state != 0)
        });
        if melee_active {
            self.melee_activity_until_ms = now_ms + MELEE_ACTIVITY_WINDOW_MS;
        }

        let Some(previous) = self.previous.take() else {
            self.previous = Some(TrackedPawn {
                address: snapshot.address,
                alive,
                interrupted: snapshot.interrupted,
                damage_taken_time: snapshot.damage_taken_time,
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
            self.previous = Some(TrackedPawn {
                address: snapshot.address,
                alive,
                interrupted: snapshot.interrupted,
                damage_taken_time: snapshot.damage_taken_time,
                abilities,
            });
            return events;
        }

        if previous.alive && !alive {
            events.push(self.emit(EventKind::Death, now_ms));
            self.ability_settle_until_ms = now_ms + ABILITY_SETTLE_MS;
        } else if !previous.alive && alive {
            events.push(self.emit(EventKind::Respawn, now_ms));
            self.ability_settle_until_ms = now_ms + ABILITY_SETTLE_MS;
        }

        let settled = alive && now_ms >= self.ability_settle_until_ms;
        for (slot, current) in &abilities {
            let Some(tracked) = previous.abilities.get(slot) else {
                continue;
            };
            let transitions =
                ability_transitions(*slot, tracked, current, snapshot.game_time);
            if !settled {
                continue;
            }
            if transitions.used && *slot < HERO_SLOT_LIMIT {
                events.push(self.emit(EventKind::AbilityUsed { slot: *slot }, now_ms));
            }
            if transitions.ready && *slot < HERO_SLOT_LIMIT {
                events.push(self.emit(EventKind::AbilityReady { slot: *slot }, now_ms));
            }
            if transitions.got_parried {
                events.push(self.emit(EventKind::Parried, now_ms));
            }

            if *slot == self.melee_slot {
                let chain_hit = current
                    .melee_chain
                    .saturating_sub(tracked.melee_chain)
                    .min(3);
                for _ in 0..chain_hit {
                    events.push(self.emit(EventKind::PunchLanded, now_ms));
                }
            }
        }

        let parry_window_open = snapshot.abilities.iter().any(|ability| {
            ability.slot == self.melee_slot
                && ability.parry_start > snapshot.game_time - 2.0
                && ability.parry_start > 0.0
        });
        let parry_landed = !previous.interrupted
            && snapshot.interrupted
            && parry_window_open;
        if settled && parry_landed {
            events.push(self.emit(EventKind::Parry, now_ms));
        }

        let player_damaged_us =
            snapshot.damage_taken_time > previous.damage_taken_time + 0.05;
        let punch_taken = player_damaged_us && now_ms <= self.enemy_punch_until_ms;
        if settled && punch_taken {
            events.push(self.emit(EventKind::PunchTaken, now_ms));
        }

        self.previous = Some(TrackedPawn {
            address: snapshot.address,
            alive,
            interrupted: snapshot.interrupted,
            damage_taken_time: snapshot.damage_taken_time,
            abilities,
        });
        events
    }

    fn update_players(&mut self, players: &[PlayerSnapshot], now_ms: u64) -> Vec<GameEvent> {
        let mut events = Vec::new();
        if players.is_empty() {
            return events;
        }
        let mut seen = std::collections::HashSet::new();
        let enemy_punched = players.iter().any(|p| {
            let tracked = self.players.get(&p.address).map(|t| t.melee_chain);
            tracked.is_some_and(|prev| p.melee_chain.saturating_sub(prev) >= 1)
        });
        if enemy_punched {
            self.enemy_punch_until_ms = now_ms + MELEE_ACTIVITY_WINDOW_MS;
        }
        for player in players {
            seen.insert(player.address);
            let Some(&previous) = self.players.get(&player.address) else {
                self.players.insert(
                    player.address,
                    TrackedPlayer {
                        kills: player.kills,
                        assists: player.assists,
                        health: player.health,
                        melee_chain: player.melee_chain,
                    },
                );
                continue;
            };

            let clamp_delta = |was: i32, now: i32| -> u32 {
                now.saturating_sub(was).max(0) as u32
            };

            if player.is_local {
                for _ in 0..clamp_delta(previous.kills, player.kills) {
                    events.push(self.emit(EventKind::Kill, now_ms));
                }
                for _ in 0..clamp_delta(previous.assists, player.assists) {
                    events.push(self.emit(EventKind::Assist, now_ms));
                }
            } else if !player.is_local
                && previous.health - player.health >= 1
                && now_ms <= self.melee_activity_until_ms
            {
                events.push(self.emit(EventKind::PunchLanded, now_ms));
            }

            self.players.insert(
                player.address,
                TrackedPlayer {
                    kills: player.kills,
                    assists: player.assists,
                    health: player.health,
                    melee_chain: player.melee_chain,
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
    got_parried: bool,
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
    
    let got_parried = !previous.attack_parried
        && current.attack_parried
        && current.parry_success_end > game_time - PARRY_SUCCESS_FRESH_SECONDS
        && current.parry_success_end < game_time + 300.0;

    Transitions {
        used: charge_used
            || (!previous.cooling && current.cooling)
            || (channel_started && slot < PARRY_SLOT_MIN),
        ready: charge_ready || (previous.cooling && !current.cooling),
        got_parried,
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
            damage_taken_time: 0.0,
            abilities: Vec::new(),
        }
    }

    fn ability(slot: u8, charges: Option<i32>, cooldown_end: f32) -> AbilitySnapshot {
        AbilitySnapshot {
            slot,
            charges,
            cooldown_end,
            channeling: false,
            attack_parried: false,
            parry_start: 0.0,
            parry_success_end: 0.0,
            melee_state: 0,
            melee_chain: 0,
        }
    }

    fn melee(slot: u8, chain: u32, channeling: bool) -> AbilitySnapshot {
        let mut ability = ability(slot, None, 0.0);
        ability.melee_chain = chain;
        ability.channeling = channeling;
        ability
    }

    fn parry_window(slot: u8, game_time: f32) -> AbilitySnapshot {
        let mut ability = ability(slot, None, 0.0);
        ability.parry_start = game_time; // window just opened
        ability.parry_success_end = 0.0;
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
            melee_chain: 0,
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

    fn kinds(events: &[GameEvent]) -> Vec<EventKind> {
        events.iter().map(|event| event.kind).collect()
    }

    #[test]
    fn first_snapshot_baselines_without_events() {
        let mut monitor = Monitor::new();
        assert!(monitor.update(track(vec![ability(0, None, 0.0)], 100.0), 1000).is_empty());
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
        assert_eq!(kinds(&monitor.update(bare(fresh), 5000)), vec![EventKind::Respawn]);
    }

    #[test]
    fn every_cooldown_emits_used_then_ready() {
        let mut monitor = Monitor::new();
        monitor.update(track(vec![ability(2, None, 0.0)], 100.0), 1000);

        // First cast: cooldown becomes active relative to the game clock.
        let used = monitor.update(track(vec![ability(2, None, 125.0)], 100.0), 1033);
        assert_eq!(kinds(&used), vec![EventKind::AbilityUsed { slot: 2 }]);

        // Cooldown expires when the game clock passes the end.
        let ready = monitor.update(track(vec![ability(2, None, 125.0)], 125.5), 9000);
        assert_eq!(kinds(&ready), vec![EventKind::AbilityReady { slot: 2 }]);

        // Second cast: the used/ready pair must fire again (regression for the
        // once-only ready bug).
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
        // Field unchanged, clock advancing: nothing new.
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
        // All abilities off cooldown after respawn: must not emit a burst.
        let events = monitor.update(track(vec![ability(0, Some(3), 0.0)], 100.0), 3200);
        assert!(events.is_empty());
        // Once the settle window passes, transitions work again.
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
    fn attack_parried_edge_emits_parried() {
        let mut monitor = Monitor::new();
        monitor.update(track(vec![ability(22, None, 0.0)], 100.0), 1000);
        let mut parried = ability(22, None, 0.0);
        parried.attack_parried = true;
        parried.parry_success_end = 105.0; // fresh success timestamp
        let events = monitor.update(track(vec![parried], 103.0), 1033);
        assert_eq!(kinds(&events), vec![EventKind::Parried]);

        // Staying true must not re-fire.
        let events = monitor.update(track(vec![parried], 104.0), 1100);
        assert!(events.is_empty());
    }

    #[test]
    fn stale_parry_success_timestamp_does_not_emit() {
        let mut monitor = Monitor::new();
        monitor.update(track(vec![ability(22, None, 0.0)], 100.0), 1000);
        let mut garbage = ability(22, None, 0.0);
        garbage.attack_parried = true;
        garbage.parry_success_end = 0.0; // not a real success timestamp
        let events = monitor.update(track(vec![garbage], 100.0), 1033);
        assert!(events.is_empty());
    }

    #[test]
    fn interrupt_during_parry_window_emits_parry() {
        let mut monitor = Monitor::new();
        monitor.update(track(vec![parry_window(22, 100.0)], 100.0), 1000);

        // Interrupt flag rises while our parry window is open: caught one.
        // The ability entity stays in the snapshot with its window timestamp.
        let mut stunned = pawn(true);
        stunned.interrupted = true;
        stunned.abilities = vec![parry_window(22, 100.0)];
        let events = monitor.update(bare(stunned), 1200);
        assert_eq!(kinds(&events), vec![EventKind::Parry]);
    }

    #[test]
    fn interrupt_from_a_punch_does_not_emit_parry() {
        let mut monitor = Monitor::new();
        // Melee window active but no parry window opened (plain punch).
        monitor.update(track(vec![melee(22, 0, true)], 100.0), 1000);
        let mut stunned = pawn(true);
        stunned.interrupted = true;
        let events = monitor.update(bare(stunned), 1200);
        assert!(events.is_empty());
    }

    #[test]
    fn interrupt_without_recent_melee_does_not_emit_parry() {
        let mut monitor = Monitor::new();
        monitor.update(track(vec![ability(0, None, 0.0)], 100.0), 1000);
        let mut stunned = pawn(true);
        stunned.interrupted = true;
        let events = monitor.update(bare(stunned), 1200);
        assert!(events.is_empty());
    }

    #[test]
    fn melee_chain_increment_emits_punch_landed() {
        let mut monitor = Monitor::new();
        monitor.update(track(vec![melee(22, 0, true)], 100.0), 1000);
        let events = monitor.update(track(vec![melee(22, 1, true)], 100.0), 1033);
        assert_eq!(kinds(&events), vec![EventKind::PunchLanded]);
        // Chain resets to 0 between swings must not emit.
        let events = monitor.update(track(vec![melee(22, 0, true)], 100.0), 1100);
        assert!(events.is_empty());
    }

    #[test]
    fn enemy_health_drop_during_our_melee_emits_punch_landed() {
        let mut monitor = Monitor::new();
        monitor.update(Snapshot {
            pawn: Some(pawn(true)),
            players: vec![player(0x10, true, 0, 0), player(0x20, false, 0, 0)],
        }, 1000);
        monitor.update(track(vec![melee(22, 0, true)], 100.0), 1033);

        let mut hit = player(0x20, false, 0, 0);
        hit.health = 420;
        let events = monitor.update(Snapshot {
            pawn: Some(pawn(true)),
            players: vec![player(0x10, true, 0, 0), hit],
        }, 1066);
        assert_eq!(kinds(&events), vec![EventKind::PunchLanded]);

        // No melee activity: enemy damage is not our punch.
        let mut hit_again = player(0x20, false, 0, 0);
        hit_again.health = 300;
        let events = monitor.update(Snapshot {
            pawn: Some(pawn(true)),
            players: vec![player(0x10, true, 0, 0), hit_again],
        }, 5000);
        assert!(events.is_empty());
    }

    #[test]
    fn player_damage_during_enemy_melee_emits_punch_taken() {
        let mut monitor = Monitor::new();
        let mut enemy = player(0x20, false, 0, 0);
        enemy.melee_chain = 0;
        monitor.update(Snapshot {
            pawn: Some(pawn(true)),
            players: vec![player(0x10, true, 0, 0), enemy],
        }, 1000);

        // The enemy's melee chain advances and we take player damage: punched.
        let mut swinging = player(0x20, false, 0, 0);
        swinging.melee_chain = 1;
        let mut punched = pawn(true);
        punched.health = 380;
        punched.damage_taken_time = 100.2;
        let events = monitor.update(Snapshot {
            pawn: Some(punched),
            players: vec![player(0x10, true, 0, 0), swinging],
        }, 1033);
        assert_eq!(kinds(&events), vec![EventKind::PunchTaken]);

        // Player damage without enemy melee activity: not a punch (bullets).
        let idle_enemy = player(0x20, false, 0, 0);
        let mut shot = pawn(true);
        shot.health = 300;
        shot.damage_taken_time = 100.5;
        let events = monitor.update(Snapshot {
            pawn: Some(shot),
            players: vec![player(0x10, true, 0, 0), idle_enemy],
        }, 5000);
        assert!(events.is_empty());
    }

    #[test]
    fn local_player_kill_and_assist_increments_emit_events() {
        let mut monitor = Monitor::new();
        monitor.update(Snapshot {
            pawn: Some(pawn(true)),
            players: vec![player(0x10, true, 0, 0), player(0x20, false, 0, 0)],
        }, 1000);

        let events = monitor.update(Snapshot {
            pawn: Some(pawn(true)),
            players: vec![player(0x10, true, 2, 1), player(0x20, false, 0, 0)],
        }, 1100);
        assert_eq!(
            kinds(&events),
            vec![EventKind::Kill, EventKind::Kill, EventKind::Assist]
        );
    }

    #[test]
    fn other_players_kills_do_not_emit() {
        let mut monitor = Monitor::new();
        monitor.update(Snapshot {
            pawn: Some(pawn(true)),
            players: vec![player(0x10, true, 0, 0), player(0x20, false, 0, 0)],
        }, 1000);
        let events = monitor.update(Snapshot {
            pawn: Some(pawn(true)),
            players: vec![player(0x10, true, 0, 0), player(0x20, false, 3, 0)],
        }, 1100);
        assert!(events.is_empty());
    }

    #[test]
    fn new_players_baseline_without_events() {
        let mut monitor = Monitor::new();
        let events = monitor.update(Snapshot {
            pawn: Some(pawn(true)),
            players: vec![player(0x10, true, 9, 7)],
        }, 1000);
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
