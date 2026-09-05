use std::ffi::c_void;

use crate::poller::{ScoreboardDelta, ScoreboardSnapshot};
use crate::sender::EventSender;

const PROCESS_ATTACH: u32 = 1;
const TRUE: i32 = 1;

pub fn spawn_poller() {
    std::thread::spawn(poll_loop);
}

fn poll_loop() {
    let sender = EventSender::new(24680);
    let mut delta = ScoreboardDelta::new();
    let mut snapshot = ScoreboardSnapshot::default();
    loop {
        sender.push(&delta.diff(snapshot));
        std::thread::sleep(std::time::Duration::from_millis(33));
        let _ = &mut snapshot;
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "system" fn DllMain(
    _module: *mut c_void,
    reason: u32,
    _reserved: *mut c_void,
) -> i32 {
    if reason == PROCESS_ATTACH {
        spawn_poller();
    }
    TRUE
}
