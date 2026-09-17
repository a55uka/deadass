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

fn poll_loop() {
    let offsets = Offsets::load();
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
    loop {
        std::thread::sleep(interval);
        if !client_announced {
            let base = super::game::client_base();
            if base != 0 {
                debug_log::log(&format!("client.dll found at {base:#x}"));
                client_announced = true;
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
