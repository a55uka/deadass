use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::os::unix::fs::FileExt;

#[derive(Debug, Clone)]
pub struct MemoryRegion {
    pub start: u64,
    pub end: u64,
    pub pathname: Option<String>,
}

pub fn client_module_base(pid: u32, module: &str) -> Option<u64> {
    mapped_regions(pid)
        .into_iter()
        .filter(|region| {
            region
                .pathname
                .as_deref()
                .is_some_and(|path| path.contains(module))
        })
        .map(|region| region.start)
        .min()
}

pub fn mapped_regions(pid: u32) -> Vec<MemoryRegion> {
    let Ok(maps) = std::fs::read_to_string(format!("/proc/{pid}/maps")) else {
        return Vec::new();
    };
    maps.lines().filter_map(parse_maps_line).collect()
}

fn parse_maps_line(line: &str) -> Option<MemoryRegion> {
    let mut parts = line.split_whitespace();
    let range = parts.next()?;
    let (start_hex, end_hex) = range.split_once('-')?;
    let start = u64::from_str_radix(start_hex, 16).ok()?;
    let end = u64::from_str_radix(end_hex, 16).ok()?;
    let pathname = parts
        .last()
        .filter(|tail| tail.starts_with('/'))
        .map(str::to_string);
    Some(MemoryRegion {
        start,
        end,
        pathname,
    })
}

pub fn read_memory(pid: u32, address: u64, out: &mut [u8]) -> std::io::Result<()> {
    let file = File::open(format!("/proc/{pid}/mem"))?;
    file.read_exact_at(out, address)
}

pub fn read_pointer_chain(pid: u32, base: u64, offsets: &[u64]) -> Option<u64> {
    let mut address = base;
    for offset in offsets {
        let mut raw = [0u8; 8];
        read_memory(pid, address + offset, &mut raw).ok()?;
        address = u64::from_le_bytes(raw);
        if address == 0 {
            return None;
        }
    }
    Some(address)
}

pub fn read_u32(pid: u32, address: u64) -> Option<u32> {
    let mut raw = [0u8; 4];
    read_memory(pid, address, &mut raw).ok()?;
    Some(u32::from_le_bytes(raw))
}

pub fn read_i32(pid: u32, address: u64) -> Option<i32> {
    let mut raw = [0u8; 4];
    read_memory(pid, address, &mut raw).ok()?;
    Some(i32::from_le_bytes(raw))
}

#[allow(dead_code)]
fn seek_read(pid: u32, address: u64, out: &mut [u8]) -> std::io::Result<()> {
    let mut file = File::open(format!("/proc/{pid}/mem"))?;
    file.seek(SeekFrom::Start(address))?;
    file.read_exact(out)
}
