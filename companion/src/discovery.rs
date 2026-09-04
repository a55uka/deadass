use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct DiscoveredGame {
    pub pid: u32,
    pub exe: PathBuf,
    pub prefix: Option<PathBuf>,
}

pub fn discover_deadlock() -> Option<DiscoveredGame> {
    scan_processes().into_iter().find(is_deadlock)
}

fn is_deadlock(candidate: &DiscoveredGame) -> bool {
    candidate
        .exe
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case("deadlock.exe"))
}

fn scan_processes() -> Vec<DiscoveredGame> {
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return found;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(pid) = name.to_str().and_then(|raw| raw.parse::<u32>().ok()) else {
            continue;
        };
        if let Some(game) = describe_process(pid) {
            found.push(game);
        }
    }
    found
}

fn describe_process(pid: u32) -> Option<DiscoveredGame> {
    let exe = std::fs::read_link(format!("/proc/{pid}/exe")).ok()?;
    let prefix = prefix_from_environ(pid).or(prefix_from_cwd(pid));
    Some(DiscoveredGame { pid, exe, prefix })
}

fn prefix_from_environ(pid: u32) -> Option<PathBuf> {
    let raw = std::fs::read(format!("/proc/{pid}/environ")).ok()?;
    let mut compat_data = None;
    let mut wine_prefix = None;
    for entry in raw.split(|byte| *byte == 0) {
        let Ok(text) = std::str::from_utf8(entry) else {
            continue;
        };
        if let Some(value) = text.strip_prefix("STEAM_COMPAT_DATA_PATH=") {
            compat_data = Some(PathBuf::from(value).join("pfx"));
        }
        if let Some(value) = text.strip_prefix("WINEPREFIX=") {
            wine_prefix = Some(PathBuf::from(value));
        }
    }
    wine_prefix.or(compat_data)
}

fn prefix_from_cwd(pid: u32) -> Option<PathBuf> {
    std::fs::read_link(format!("/proc/{pid}/cwd")).ok()
}

pub fn to_wine_path(linux_path: &std::path::Path) -> String {
    let rendered = linux_path.to_string_lossy().replace('/', "\\");
    format!("Z:{rendered}")
}
