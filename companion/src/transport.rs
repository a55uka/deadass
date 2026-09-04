use deadass_shared::{GameEvent, InputMode, TriggerKind};
use std::collections::{HashMap, VecDeque};
use tokio::sync::{broadcast, mpsc};

#[derive(Debug, Clone)]
pub struct BusMessage {
    pub event: GameEvent,
    pub accepted: bool,
}

pub struct EventBus {
    inbound: mpsc::UnboundedSender<GameEvent>,
    outbound: broadcast::Sender<BusMessage>,
}

impl EventBus {
    pub fn new(buffer: usize) -> (Self, EventIngress, EventOutlet) {
        let (inbound_tx, inbound_rx) = mpsc::unbounded_channel();
        let (outbound_tx, _) = broadcast::channel(buffer.max(16));
        let bus = Self {
            inbound: inbound_tx.clone(),
            outbound: outbound_tx.clone(),
        };
        (
            bus,
            EventIngress {
                receiver: inbound_rx,
                broadcaster: outbound_tx.clone(),
            },
            EventOutlet {
                receiver: outbound_tx.subscribe(),
            },
        )
    }

    pub fn sender(&self) -> mpsc::UnboundedSender<GameEvent> {
        self.inbound.clone()
    }

    pub fn subscribe(&self) -> broadcast::Receiver<BusMessage> {
        self.outbound.subscribe()
    }
}

pub struct EventIngress {
    receiver: mpsc::UnboundedReceiver<GameEvent>,
    broadcaster: broadcast::Sender<BusMessage>,
}

impl EventIngress {
    pub async fn run(mut self, mode: InputMode) {
        let mut dedup = EventDeduplicator::new(200);
        while let Some(event) = self.receiver.recv().await {
            if !mode_accepts(mode, event) {
                continue;
            }
            if !dedup.should_emit(event) {
                continue;
            }
            let _ = self.broadcaster.send(BusMessage {
                event,
                accepted: true,
            });
        }
    }
}

pub struct EventOutlet {
    receiver: broadcast::Receiver<BusMessage>,
}

impl EventOutlet {
    pub async fn next(&mut self) -> Option<GameEvent> {
        self.receiver.recv().await.ok().map(|message| message.event)
    }
}

pub struct EventDeduplicator {
    window_ms: u64,
    recent: VecDeque<(TriggerKind, u64)>,
}

impl EventDeduplicator {
    pub fn new(window_ms: u64) -> Self {
        Self {
            window_ms,
            recent: VecDeque::new(),
        }
    }

    pub fn should_emit(&mut self, event: GameEvent) -> bool {
        let trigger = TriggerKind::from_event(event.kind);
        self.recent
            .retain(|(_, seen_at)| event.wall_time_ms.saturating_sub(*seen_at) <= self.window_ms);
        if self.recent.iter().any(|(known, _)| *known == trigger) {
            return false;
        }
        self.recent.push_back((trigger, event.wall_time_ms));
        if self.recent.len() > 64 {
            self.recent.pop_front();
        }
        true
    }
}

fn mode_accepts(mode: InputMode, event: GameEvent) -> bool {
    use deadass_shared::EventSource;
    match mode {
        InputMode::Auto => true,
        InputMode::ModOnly => matches!(event.source, EventSource::Mod),
        InputMode::MemoryOnly => matches!(event.source, EventSource::Dll | EventSource::External),
    }
}

pub fn parse_newline_events(raw: &[u8]) -> Vec<GameEvent> {
    raw.split(|byte| *byte == b'\n')
        .filter_map(|line| {
            if line.is_empty() {
                return None;
            }
            serde_json::from_slice(line).ok()
        })
        .collect()
}

#[allow(dead_code)]
fn pending_triggers(events: &[GameEvent]) -> HashMap<TriggerKind, usize> {
    let mut counts = HashMap::new();
    for event in events {
        *counts
            .entry(TriggerKind::from_event(event.kind))
            .or_insert(0) += 1;
    }
    counts
}
