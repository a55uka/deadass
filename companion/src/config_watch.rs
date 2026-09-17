use crate::ui::AppState;
use deadass_shared::{AppConfig, TriggerConfig};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

const WATCH_POLL: Duration = Duration::from_millis(500);
const MAX_TRIGGER_LINES: usize = 6;

pub fn spawn(path: PathBuf, state: Arc<Mutex<AppState>>) -> tokio::task::JoinHandle<()> {
    let last_modified = modified_at(&path);
    tokio::spawn(async move {
        let mut last = last_modified;
        loop {
            tokio::time::sleep(WATCH_POLL).await;
            let modified = modified_at(&path);
            if modified == last {
                continue;
            }
            last = modified;
            if modified.is_some() {
                reload_from_disk(&path, &state).await;
            }
        }
    })
}

fn modified_at(path: &Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(path).and_then(|meta| meta.modified()).ok()
}

async fn reload_from_disk(path: &Path, state: &Arc<Mutex<AppState>>) {
    let parsed: Option<AppConfig> = std::fs::read_to_string(path)
        .ok()
        .and_then(|raw| toml::from_str(&raw).ok())
        .map(AppConfig::normalized);
    let Some(fresh) = parsed else {
        state.lock().await.push_log(format!(
            "config {} could not be parsed; keeping the running config",
            path.display()
        ));
        return;
    };
    let mut locked = state.lock().await;
    if locked.config == fresh {
        return;
    }
    let previous = locked.replace_config(fresh);
    locked.push_log(format!("config reloaded from {}", path.display()));
    for line in describe_change(&previous, &locked.config) {
        locked.push_log(line);
    }
}

/// Human-readable diff of two configs, one line per changed field.
pub fn describe_change(old: &AppConfig, new: &AppConfig) -> Vec<String> {
    let mut lines = Vec::new();
    let mut field = |label: &str, change: Option<String>| {
        if let Some(detail) = change {
            lines.push(format!("{label} {detail}"));
        }
    };
    field(
        "master_gain",
        changed_f64(old.master_gain, new.master_gain),
    );
    field(
        "max_strength_cap",
        changed_f64(old.max_strength_cap, new.max_strength_cap),
    );
    field(
        "mute_while_dead",
        changed_bool(old.mute_while_dead, new.mute_while_dead),
    );
    field(
        "debug_logging",
        changed_bool(old.debug_logging, new.debug_logging),
    );
    field(
        "mod_http_port",
        changed_u64(old.mod_http_port, new.mod_http_port),
    );
    field(
        "dll_event_port",
        changed_u64(old.dll_event_port, new.dll_event_port),
    );
    field(
        "buttplug_ws_url",
        (old.buttplug_ws_url != new.buttplug_ws_url)
            .then(|| format!("{} -> {}", quote_none(&old.buttplug_ws_url), quote_none(&new.buttplug_ws_url))),
    );
    field(
        "dll_path",
        match (old.dll_path.as_deref(), new.dll_path.as_deref()) {
            (old, new) if old == new => None,
            (old, new) => Some(format!(
                "{} -> {}",
                old.unwrap_or("(default)"),
                new.unwrap_or("(default)")
            )),
        },
    );
    field(
        "data_source",
        (old.data_source != new.data_source)
            .then(|| format!("{} -> {}", old.data_source.as_str(), new.data_source.as_str())),
    );

    let mut trigger_lines = Vec::new();
    let mut keys: Vec<_> = old
        .triggers
        .keys()
        .chain(new.triggers.keys())
        .collect();
    keys.sort_by_key(|kind| kind.to_string());
    keys.dedup();
    for key in keys {
        let old_rule = old.triggers.get(key);
        let new_rule = new.triggers.get(key);
        if old_rule == new_rule {
            continue;
        }
        let empty = TriggerConfig::default();
        let old_rule = old_rule.unwrap_or(&empty);
        let new_rule = new_rule.unwrap_or(&empty);
        if let Some(detail) = describe_trigger(old_rule, new_rule) {
            trigger_lines.push(format!("{} {}", key, detail));
        }
    }
    if trigger_lines.len() > MAX_TRIGGER_LINES {
        let overflow = trigger_lines.len() - MAX_TRIGGER_LINES;
        trigger_lines.truncate(MAX_TRIGGER_LINES);
        trigger_lines.push(format!("... and {overflow} more trigger changes"));
    }
    lines.append(&mut trigger_lines);
    lines
}

fn describe_trigger(old: &TriggerConfig, new: &TriggerConfig) -> Option<String> {
    let mut parts = Vec::new();
    if old.enabled != new.enabled {
        parts.push(format!("enabled {}", on_off(old.enabled, new.enabled)));
    }
    if old.strength != new.strength {
        parts.push(format!("strength {}", changed_f64(old.strength, new.strength).unwrap_or_default()));
    }
    if old.duration_ms != new.duration_ms {
        parts.push(format!(
            "duration_ms {} -> {}",
            old.duration_ms, new.duration_ms
        ));
    }
    if old.retrigger_cooldown_ms != new.retrigger_cooldown_ms {
        parts.push(format!(
            "cooldown_ms {} -> {}",
            old.retrigger_cooldown_ms, new.retrigger_cooldown_ms
        ));
    }
    if old.pattern != new.pattern {
        parts.push(format!("pattern {:?} -> {:?}", old.pattern, new.pattern));
    }
    (!parts.is_empty()).then(|| parts.join(", "))
}

fn changed_f64(old: f64, new: f64) -> Option<String> {
    (old != new).then(|| format!("{old:.2} -> {new:.2}"))
}

fn changed_u64(old: u16, new: u16) -> Option<String> {
    (old != new).then(|| format!("{old} -> {new}"))
}

fn changed_bool(old: bool, new: bool) -> Option<String> {
    (old != new).then(|| on_off(old, new))
}

fn on_off(old: bool, new: bool) -> String {
    format!("{} -> {}", if old { "on" } else { "off" }, if new { "on" } else { "off" })
}

fn quote_none(value: &str) -> &str {
    if value.is_empty() {
        "(empty)"
    } else {
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadass_shared::{Pattern, TriggerKind};

    fn config_with(mut edit: impl FnMut(&mut AppConfig)) -> AppConfig {
        let mut config = AppConfig::default();
        edit(&mut config);
        config
    }

    #[test]
    fn identical_configs_describe_nothing() {
        let config = AppConfig::default();
        assert!(describe_change(&config, &config).is_empty());
    }

    #[test]
    fn scalar_changes_are_described() {
        let old = AppConfig::default();
        let new = config_with(|config| {
            config.master_gain = 0.5;
            config.debug_logging = true;
        });
        let lines = describe_change(&old, &new);
        assert_eq!(lines, ["master_gain 1.00 -> 0.50", "debug_logging off -> on"]);
    }

    #[test]
    fn trigger_changes_are_described_and_capped() {
        let old = AppConfig::default();
        let new = config_with(|config| {
            for slot in 0..8u8 {
                let rule = config
                    .triggers
                    .entry(TriggerKind::AbilityUsed { slot })
                    .or_default();
                rule.strength = 0.1;
                rule.pattern = Pattern::Ramp;
            }
        });
        let lines = describe_change(&old, &new);
        let tail = lines.last().expect("changes listed");
        assert!(tail.starts_with("... and "));
        assert_eq!(lines.len(), MAX_TRIGGER_LINES + 1);
    }
}
