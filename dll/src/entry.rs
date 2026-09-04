#[cfg(windows)]
use windows_sys::Win32::Foundation::{BOOL, HMODULE, TRUE};
#[cfg(windows)]
use windows_sys::Win32::System::Threading::{CreateThread, THREAD_CREATION_FLAGS};

use crate::poller::{ScoreboardDelta, ScoreboardSnapshot};
use crate::sender::EventSender;

pub struct DllEntry;

impl DllEntry {
    #[cfg(windows)]
    pub unsafe fn attach() -> BOOL {
        let mut _thread_id = 0u32;
        CreateThread(
            std::ptr::null(),
            0,
            Some(poll_thread),
            std::ptr::null(),
            THREAD_CREATION_FLAGS(0),
            &mut _thread_id,
        );
        TRUE
    }
}

#[cfg(windows)]
unsafe extern "system" fn poll_thread(_parameter: *mut std::ffi::c_void) -> u32 {
    let sender = EventSender::new(24680);
    let mut delta = ScoreboardDelta::new();
    let mut last_snapshot = ScoreboardSnapshot::default();
    loop {
        let events = delta.diff(last_snapshot);
        sender.push(&events);
        windows_sys::Win32::System::Threading::Sleep(33);
        let _ = &mut last_snapshot;
    }
}

#[cfg(windows)]
#[no_mangle]
pub unsafe extern "system" fn DllMain(
    _module: HMODULE,
    reason: u32,
    _reserved: *mut std::ffi::c_void,
) -> BOOL {
    const PROCESS_ATTACH: u32 = 1;
    if reason == PROCESS_ATTACH {
        return DllEntry::attach();
    }
    TRUE
}
