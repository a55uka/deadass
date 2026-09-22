use std::sync::OnceLock;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;

use super::memory::{read_bytes, read_u64};
use crate::offsets::Offsets;

/// Highest entity index we will ever chase when resolving ability handles
pub(super) const MAX_ENTITY_INDEX: u64 = 1 << 15;

pub fn client_base() -> u64 {
    let name: Vec<u16> = "client.dll\0".encode_utf16().collect();
    let handle = unsafe { GetModuleHandleW(name.as_ptr()) };
    handle as u64
}

fn image_bounds() -> (u64, u64) {
    static BOUNDS: OnceLock<(u64, u64)> = OnceLock::new();
    if let Some(bounds) = BOUNDS.get() {
        return *bounds;
    }
    let base = client_base();
    if base == 0 {
        return (0, 0);
    }
    // PE header: e_lfanew at +0x3C, SizeOfImage optional header +0x50
    let e_lfanew = u32_at(base + 0x3C).unwrap_or(0) as u64;
    let size = u32_at(base + e_lfanew + 0x50).unwrap_or(0) as u64;
    if size == 0 || size > (1 << 30) {
        return (0, 0);
    }
    let bounds = (base, base.saturating_add(size));
    let _ = BOUNDS.set(bounds);
    bounds
}

fn u32_at(address: u64) -> Option<u32> {
    let slice = unsafe { std::slice::from_raw_parts(address as *const u8, 4) };
    Some(u32::from_le_bytes(slice.try_into().ok()?))
}

pub(super) fn in_client_image(addr: u64) -> bool {
    let (base, end) = image_bounds();
    base != 0 && addr >= base && addr < end
}

pub(super) fn entity_pointer_valid(entity: u64) -> bool {
    entity_pointer_vtable(entity).is_some()
}

pub(super) fn entity_pointer_vtable(entity: u64) -> Option<u64> {
    let raw = read_bytes(entity, 8)?;
    let vtable = u64::from_le_bytes(raw.try_into().ok()?);
    in_client_image(vtable).then_some(vtable)
}

/// Resolve a Source 2 entity handle (low 15 bits) through the
/// entity system's chunked list: chunk array at `entity_system +
/// entity_chunk_array`, 8-byte chunk pointers, `entity_stride`-byte slots
pub(super) fn resolve_entity(handle: u32, offsets: &Offsets, entity_system: u64) -> Option<u64> {
    let index = (handle & 0x7FFF) as u64;
    if index >= MAX_ENTITY_INDEX || entity_system == 0 {
        return None;
    }
    let chunk_array = entity_system.checked_add(offsets.entity_chunk_array)?;
    let chunk =
        read_u64(chunk_array.checked_add((index / offsets.entity_chunk_size).checked_mul(8)?)?)?;
    if chunk == 0 {
        return None;
    }
    let slot = index % offsets.entity_chunk_size;
    read_u64(chunk.checked_add(slot.checked_mul(offsets.entity_stride)?)?)
}
