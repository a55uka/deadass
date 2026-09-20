use std::ffi::c_void;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::debug_log;
use super::diff::Monitor;
use super::offsets::Offsets;
use super::sender::EventSender;

const PROCESS_ATTACH: u32 = 1;
const TRUE: i32 = 1;

static TICKS: AtomicU64 = AtomicU64::new(0);

pub fn spawn_poller() {
    std::thread::spawn(poll_loop);
}

/// Retry the schema-backed offset load every N ticks (~1s) until it
/// succeeds or attempts run out; schema needs client.dll loaded, which lags
/// injection.
const SCHEMA_RETRY_TICKS: u64 = 30;
const SCHEMA_RETRY_MAX: u32 = 20;

fn poll_loop() {
    let mut offsets = Offsets::load();
    let sender = EventSender::new(offsets.dll_port);
    let interval = Duration::from_millis(offsets.poll_interval_ms);
    let mut monitor = Monitor::new();
    monitor.set_melee_slot(offsets.melee_slot);
    debug_log::log(&format!(
        "poller up: sending to {} every {}ms",
        sender.endpoint(),
        offsets.poll_interval_ms
    ));

    let mut client_announced = false;
    let mut pawn_seen: Option<u64> = None;
    let mut players_log = String::new();
    let mut tick: u64 = 0;
    let mut schema_attempts: u32 = 1; // Offsets::load already tried once
    loop {
        std::thread::sleep(interval);
        tick += 1;
        if !client_announced {
            let base = super::game::client_base();
            if base != 0 {
                debug_log::log(&format!("client.dll found at {base:#x}"));
                client_announced = true;
            }
        }

        // The schema system only exists once the game is up; reload (which
        // re-applies the toml overlay on top) until it resolves the fields.
        #[cfg(windows)]
        if !offsets.schema_resolved
            && tick % SCHEMA_RETRY_TICKS == 0
            && schema_attempts < SCHEMA_RETRY_MAX
        {
            schema_attempts += 1;
            let fresh = Offsets::load();
            if fresh.schema_resolved {
                debug_log::log("schema offsets applied on retry");
                offsets = fresh;
                monitor.set_melee_slot(offsets.melee_slot);
            }
        }

        let snapshot = super::game::snapshot(&offsets);
        match &snapshot.pawn {
            Some(current) => {
                if pawn_seen != Some(current.address) {
                    debug_log::log(&format!(
                        "local pawn {:#x} health={} life_state={} abilities={:?}",
                        current.address, current.health, current.life_state, current.abilities
                    ));
                }
                pawn_seen = Some(current.address);
            }
            None => {
                if pawn_seen.is_some() {
                    debug_log::log("local pawn lost (left match or back in menu)");
                    pawn_seen = None;
                }
            }
        }
        if !snapshot.players.is_empty() {
            let table: Vec<String> = snapshot
                .players
                .iter()
                .map(|p| {
                    format!(
                        "{}{:#x}:hero={} k/a/d={}/{}/{} streak={} alive={}",
                        if p.is_local { "*0x" } else { "0x" },
                        p.address,
                        p.hero_id,
                        p.kills,
                        p.assists,
                        p.deaths,
                        p.kill_streak,
                        p.alive
                    )
                })
                .collect();
            let table = table.join(" | ");
            if table != players_log {
                debug_log::log(&format!("players: {table}"));
                players_log = table;
            }
        }

        let events = monitor.update(snapshot, now_ms());
        if !events.is_empty() {
            debug_log::log(&format!("sending {} events", events.len()));
        }
        sender.push(&events);
        TICKS.fetch_add(1, Ordering::Relaxed);
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(windows)]
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub unsafe extern "system" fn DllMain(
    module: *mut c_void,
    reason: u32,
    _reserved: *mut c_void,
) -> i32 {
    use windows_sys::Win32::System::LibraryLoader::DisableThreadLibraryCalls;

    if reason == PROCESS_ATTACH {
        unsafe { DisableThreadLibraryCalls(module) };
        spawn_poller();
    }
    TRUE
}
