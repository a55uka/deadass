use crate::dedup::EventDeduplicator;
use deadass_shared::GameEvent;
use std::time::Duration;
use tokio::sync::{broadcast, mpsc};

const DEDUP_WINDOW: Duration = Duration::from_millis(200);
const MIN_BROADCAST_BUFFER: usize = 16;

#[derive(Clone, Copy)]
pub struct BusMessage {
    pub event: GameEvent,
}

pub struct EventBus {
    inbound: mpsc::UnboundedSender<GameEvent>,
}

impl EventBus {
    pub fn new(buffer: usize) -> (Self, EventIngress, EventOutlet) {
        let (inbound_tx, inbound_rx) = mpsc::unbounded_channel();
        let (outbound_tx, _) = broadcast::channel(buffer.max(MIN_BROADCAST_BUFFER));
        let bus = Self {
            inbound: inbound_tx,
        };
        (
            bus,
            EventIngress::new(inbound_rx, outbound_tx.clone()),
            EventOutlet::new(outbound_tx.subscribe()),
        )
    }

    pub fn sender(&self) -> mpsc::UnboundedSender<GameEvent> {
        self.inbound.clone()
    }
}

pub struct EventIngress {
    receiver: mpsc::UnboundedReceiver<GameEvent>,
    broadcaster: broadcast::Sender<BusMessage>,
}

impl EventIngress {
    fn new(
        receiver: mpsc::UnboundedReceiver<GameEvent>,
        broadcaster: broadcast::Sender<BusMessage>,
    ) -> Self {
        Self {
            receiver,
            broadcaster,
        }
    }

    pub async fn run(mut self) {
        let mut dedup = EventDeduplicator::new(DEDUP_WINDOW);
        while let Some(event) = self.receiver.recv().await {
            if !dedup.should_emit(event) {
                continue;
            }
            let _ = self.broadcaster.send(BusMessage { event });
        }
    }
}

pub struct EventOutlet {
    receiver: broadcast::Receiver<BusMessage>,
}

impl EventOutlet {
    fn new(receiver: broadcast::Receiver<BusMessage>) -> Self {
        Self { receiver }
    }

    pub async fn next(&mut self) -> Option<GameEvent> {
        self.receiver.recv().await.ok().map(|message| message.event)
    }
}
