use super::is_readable;

pub fn pages_committed(address: u64, len: usize) -> bool {
    use windows_sys::Win32::System::Memory::{
        MEM_COMMIT, MEMORY_BASIC_INFORMATION, PAGE_GUARD, PAGE_NOACCESS, VirtualQueryEx,
    };
    use windows_sys::Win32::System::Threading::GetCurrentProcess;

    const PAGE_GUARD_FLAG: u32 = PAGE_GUARD;
    const PAGE_NOACCESS_FLAG: u32 = PAGE_NOACCESS;

    let Some(end) = address.checked_add(len as u64) else {
        return false;
    };
    let mut cursor = address;
    let mut info: MEMORY_BASIC_INFORMATION = unsafe { std::mem::zeroed() };
    let info_size = std::mem::size_of::<MEMORY_BASIC_INFORMATION>();
    while cursor < end {
        let process = unsafe { GetCurrentProcess() };
        let written = unsafe { VirtualQueryEx(process, cursor as *const _, &mut info, info_size) };
        if written == 0 {
            return false;
        }
        let region_end = cursor.saturating_add(info.RegionSize as u64);
        if region_end <= cursor {
            return false;
        }
        if info.State != MEM_COMMIT
            || (info.Protect & PAGE_GUARD_FLAG) != 0
            || info.Protect == PAGE_NOACCESS_FLAG
        {
            return false;
        }
        cursor = region_end;
    }
    true
}

pub(super) fn read_bytes(address: u64, len: usize) -> Option<&'static [u8]> {
    if !is_readable(address, len) || !pages_committed(address, len) {
        return None;
    }
    let slice = unsafe { std::slice::from_raw_parts(address as *const u8, len) };
    Some(slice)
}

pub(super) fn read_u8(address: u64) -> Option<u8> {
    Some(read_bytes(address, 1)?[0])
}

pub(super) fn read_u16(address: u64) -> Option<u16> {
    Some(u16::from_le_bytes(read_bytes(address, 2)?.try_into().ok()?))
}

pub(super) fn read_u32(address: u64) -> Option<u32> {
    Some(u32::from_le_bytes(read_bytes(address, 4)?.try_into().ok()?))
}

pub(super) fn read_i32(address: u64) -> Option<i32> {
    Some(i32::from_le_bytes(read_bytes(address, 4)?.try_into().ok()?))
}

pub(super) fn read_f32(address: u64) -> Option<f32> {
    Some(f32::from_le_bytes(read_bytes(address, 4)?.try_into().ok()?))
}

pub(super) fn read_u64(address: u64) -> Option<u64> {
    Some(u64::from_le_bytes(read_bytes(address, 8)?.try_into().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::System::Memory::{
        MEM_COMMIT, MEM_DECOMMIT, MEM_RELEASE, MEM_RESERVE, PAGE_READWRITE, VirtualAlloc,
        VirtualFree,
    };

    #[test]
    fn freed_pages_are_not_readable() {
        unsafe {
            let page = VirtualAlloc(
                std::ptr::null(),
                0x10000,
                MEM_RESERVE | MEM_COMMIT,
                PAGE_READWRITE,
            );
            assert!(!page.is_null(), "allocation failed");
            let address = page as u64;
            assert!(pages_committed(address, 16));
            assert!(read_bytes(address, 8).is_some());

            assert_ne!(VirtualFree(page as _, 0x10000, MEM_DECOMMIT), 0);
            assert!(!pages_committed(address, 16));
            assert!(read_bytes(address, 8).is_none());

            assert_ne!(VirtualFree(page as _, 0, MEM_RELEASE), 0);
            assert!(!pages_committed(address, 16));
        }
    }
}
