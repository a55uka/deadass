use crate::pipeline::SourceGate;
use crate::ui::AppState;
use crate::ui::state::{InjectPhase, InjectStatus};
use deadass_shared::DataSource;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

pub const GAME_PROCESS: &str = "deadlock.exe";
pub const DLL_FILE_NAME: &str = "deadass_dll.dll";
const SUPERVISOR_INTERVAL: Duration = Duration::from_secs(2);

pub fn resolve_dll_path(config: &deadass_shared::AppConfig) -> PathBuf {
    if let Some(path) = config.dll_path.as_deref().filter(|raw| !raw.is_empty()) {
        return PathBuf::from(path);
    }
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
        .join(DLL_FILE_NAME)
}

pub fn spawn_supervisor(gate: SourceGate, state: Arc<Mutex<AppState>>, dll_path: PathBuf) {
    tokio::spawn(supervise(gate, state, dll_path));
}

async fn supervise(gate: SourceGate, state: Arc<Mutex<AppState>>, dll_path: PathBuf) {
    let mut previous = InjectStatus::default();
    loop {
        let next = if !gate.allows(DataSource::Dll) {
            InjectStatus::phase(InjectPhase::Idle)
        } else {
            probe(&dll_path).await
        };
        if next != previous {
            let line = describe(&next);
            let mut locked = state.lock().await;
            locked.set_inject(next.phase.clone(), next.detail.clone());
            if let Some(line) = line {
                locked.push_log(line);
            }
        }
        previous = next;
        tokio::time::sleep(SUPERVISOR_INTERVAL).await;
    }
}

fn describe(status: &InjectStatus) -> Option<String> {
    match &status.phase {
        InjectPhase::Idle => Some("dll source off".into()),
        InjectPhase::WaitingForGame => Some("waiting for deadlock.exe".into()),
        InjectPhase::Injected => Some(format!(
            "{} injected ({})",
            DLL_FILE_NAME,
            status.detail.as_deref().unwrap_or_default()
        )),
        InjectPhase::Failed => Some(format!(
            "dll injection failed: {}",
            status.detail.as_deref().unwrap_or("unknown error")
        )),
    }
}

async fn probe(dll_path: &Path) -> InjectStatus {
    if !dll_path.is_file() {
        return InjectStatus::failed(format!(
            "dll not found at {} (build it with: cargo build -p deadass-dll)",
            dll_path.display()
        ));
    }
    match find_deadlock_pid() {
        None => InjectStatus::phase(InjectPhase::WaitingForGame),
        Some(pid) => {
            if is_dll_loaded(pid) {
                InjectStatus::phase(InjectPhase::Injected)
            } else {
                match inject(pid, dll_path) {
                    Ok(()) => InjectStatus::detail(InjectPhase::Injected, format!("pid {pid}")),
                    Err(error) => InjectStatus::failed(error),
                }
            }
        }
    }
}

#[cfg(windows)]
mod windows {
    use std::path::Path;
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::Debug::WriteProcessMemory;
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, MODULEENTRY32W, Module32FirstW, Module32NextW, PROCESSENTRY32W,
        Process32FirstW, Process32NextW, TH32CS_SNAPMODULE, TH32CS_SNAPMODULE32,
        TH32CS_SNAPPROCESS,
    };
    use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
    use windows_sys::Win32::System::Memory::{
        MEM_COMMIT, MEM_RELEASE, MEM_RESERVE, PAGE_READWRITE, VirtualAllocEx, VirtualFreeEx,
    };
    use windows_sys::Win32::System::Threading::{
        CreateRemoteThread, GetExitCodeThread, OpenProcess, PROCESS_CREATE_THREAD,
        PROCESS_QUERY_INFORMATION, PROCESS_VM_OPERATION, PROCESS_VM_READ, PROCESS_VM_WRITE,
        WaitForSingleObject,
    };

    const PROCESS_PERMISSIONS: u32 = PROCESS_CREATE_THREAD
        | PROCESS_QUERY_INFORMATION
        | PROCESS_VM_OPERATION
        | PROCESS_VM_READ
        | PROCESS_VM_WRITE;

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn trimmed(raw: &[u16]) -> String {
        String::from_utf16_lossy(raw)
            .trim_end_matches('\0')
            .to_string()
    }

    pub fn find_deadlock_pid() -> Option<u32> {
        unsafe {
            let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if snapshot == INVALID_HANDLE_VALUE {
                return None;
            }
            let mut entry: PROCESSENTRY32W = std::mem::zeroed();
            entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
            let mut found = None;
            if Process32FirstW(snapshot, &mut entry) != 0 {
                loop {
                    if trimmed(&entry.szExeFile).eq_ignore_ascii_case(super::GAME_PROCESS) {
                        found = Some(entry.th32ProcessID);
                        break;
                    }
                    if Process32NextW(snapshot, &mut entry) == 0 {
                        break;
                    }
                }
            }
            CloseHandle(snapshot);
            found
        }
    }

    pub fn is_dll_loaded(pid: u32) -> bool {
        unsafe {
            let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPMODULE | TH32CS_SNAPMODULE32, pid);
            if snapshot == INVALID_HANDLE_VALUE {
                return false;
            }
            let mut entry: MODULEENTRY32W = std::mem::zeroed();
            entry.dwSize = std::mem::size_of::<MODULEENTRY32W>() as u32;
            let mut found = false;
            if Module32FirstW(snapshot, &mut entry) != 0 {
                loop {
                    if trimmed(&entry.szModule).eq_ignore_ascii_case(super::DLL_FILE_NAME) {
                        found = true;
                        break;
                    }
                    if Module32NextW(snapshot, &mut entry) == 0 {
                        break;
                    }
                }
            }
            CloseHandle(snapshot);
            found
        }
    }

    pub fn inject(pid: u32, dll_path: &Path) -> Result<(), String> {
        let path = std::fs::canonicalize(dll_path)
            .map_err(|error| format!("cannot resolve dll path: {error}"))?;
        let path = wide(&path.to_string_lossy());
        let path_bytes = path.len() * std::mem::size_of::<u16>();

        unsafe {
            let process: HANDLE = OpenProcess(PROCESS_PERMISSIONS, 0, pid);
            if process.is_null() || process == INVALID_HANDLE_VALUE {
                return Err("could not open deadlock.exe; try running deadass as admin".into());
            }
            let remote = VirtualAllocEx(
                process,
                std::ptr::null(),
                path_bytes,
                MEM_COMMIT | MEM_RESERVE,
                PAGE_READWRITE,
            );
            if remote.is_null() {
                CloseHandle(process);
                return Err("could not allocate memory in deadlock.exe".into());
            }
            let written = WriteProcessMemory(
                process,
                remote,
                path.as_ptr().cast(),
                path_bytes,
                std::ptr::null_mut(),
            );
            if written == 0 {
                cleanup(process, remote, std::ptr::null_mut());
                return Err("could not write dll path into deadlock.exe".into());
            }

            let kernel = GetModuleHandleW(wide("kernel32.dll").as_ptr());
            let Some(loader) = GetProcAddress(kernel, c"LoadLibraryW".as_ptr().cast()) else {
                cleanup(process, remote, std::ptr::null_mut());
                return Err("kernel32!LoadLibraryW not found".into());
            };
            let start: unsafe extern "system" fn(*mut core::ffi::c_void) -> u32 =
                std::mem::transmute(loader);
            let thread: HANDLE = CreateRemoteThread(
                process,
                std::ptr::null(),
                0,
                Some(start),
                remote,
                0,
                std::ptr::null_mut(),
            );
            if thread.is_null() || thread == INVALID_HANDLE_VALUE {
                cleanup(process, remote, std::ptr::null_mut());
                return Err("CreateRemoteThread failed".into());
            }
            WaitForSingleObject(thread, 10_000);
            let mut exit_code: u32 = 0;
            GetExitCodeThread(thread, &mut exit_code);
            cleanup(process, remote, thread);
            if exit_code == 0 {
                return Err("LoadLibraryW returned null (dll rejected or wrong arch?)".into());
            }
            Ok(())
        }
    }

    unsafe fn cleanup(process: HANDLE, remote: *mut core::ffi::c_void, thread: HANDLE) {
        unsafe {
            if !thread.is_null() {
                CloseHandle(thread);
            }
            if !remote.is_null() {
                VirtualFreeEx(process, remote, 0, MEM_RELEASE);
            }
            CloseHandle(process);
        }
    }
}

#[cfg(windows)]
use windows::{find_deadlock_pid, inject, is_dll_loaded};

#[cfg(not(windows))]
fn find_deadlock_pid() -> Option<u32> {
    None
}

#[cfg(not(windows))]
fn is_dll_loaded(_pid: u32) -> bool {
    false
}

#[cfg(not(windows))]
fn inject(_pid: u32, _dll_path: &Path) -> Result<(), String> {
    Err("dll injection is only supported in the Windows build".into())
}
