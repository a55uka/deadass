#[cfg(windows)]
mod entities;
#[cfg(windows)]
mod memory;
#[cfg(windows)]
mod reader;
#[cfg(not(windows))]
mod stub;

#[cfg(windows)]
pub use entities::client_base;
#[cfg(windows)]
pub use memory::pages_committed;
#[cfg(windows)]
pub use reader::snapshot;

#[cfg(not(windows))]
pub use stub::{client_base, snapshot};

const MIN_USER_ADDRESS: u64 = 0x10000;
const MAX_USER_ADDRESS: u64 = 0x7FFF_FFFE_FFFF;

pub(crate) fn is_readable(address: u64, len: usize) -> bool {
    let end = match address.checked_add(len as u64) {
        Some(end) => end,
        None => return false,
    };
    address >= MIN_USER_ADDRESS && end <= MAX_USER_ADDRESS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_x64_module_addresses_are_readable() {
        assert!(is_readable(0x7FFA_5C7E_0000, 8));
        assert!(is_readable(0x140_0000, 4));
    }

    #[test]
    fn garbage_pointers_are_rejected() {
        assert!(!is_readable(0, 8));
        assert!(!is_readable(0xFFF, 8));
        assert!(!is_readable(0xFFFF_8000_0000_0000, 8));
        assert!(!is_readable(u64::MAX, 16));
    }
}
