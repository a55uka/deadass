use deadass_shared::GameEvent;
use std::io::Write;
use std::net::TcpStream;
use std::time::Duration;

const CONNECT_TIMEOUT: Duration = Duration::from_millis(250);

pub struct EventSender {
    endpoint: String,
}

impl EventSender {
    pub fn new(port: u16) -> Self {
        Self {
            endpoint: format!("127.0.0.1:{port}"),
        }
    }

    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    pub fn encode(events: &[GameEvent]) -> Vec<u8> {
        let mut framed = Vec::new();
        for event in events {
            if let Ok(mut line) = serde_json::to_vec(event) {
                line.push(b'\n');
                framed.extend_from_slice(&line);
            }
        }
        framed
    }

    #[cfg(windows)]
    pub fn push(&self, events: &[GameEvent]) {
        if events.is_empty() {
            return;
        }
        let payload = Self::encode(events);
        let Ok(mut stream) = TcpStream::connect_timeout(
            &self
                .endpoint
                .parse()
                .unwrap_or_else(|_| "127.0.0.1:0".parse().unwrap()),
            CONNECT_TIMEOUT,
        ) else {
            return;
        };
        let _ = stream.write_all(&payload);
    }

    #[cfg(not(windows))]
    pub fn push(&self, _events: &[GameEvent]) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadass_shared::EventKind;

    #[test]
    fn encode_frames_one_json_object_per_line() {
        let events = [
            GameEvent::new(1, 42, EventKind::Kill),
            GameEvent::new(2, 43, EventKind::AbilityUsed { slot: 2 }),
        ];
        let raw = String::from_utf8(EventSender::encode(&events)).unwrap();
        let mut lines = raw.lines();
        let first: GameEvent = serde_json::from_str(lines.next().unwrap()).unwrap();
        assert_eq!(first.kind, EventKind::Kill);
        let second: GameEvent = serde_json::from_str(lines.next().unwrap()).unwrap();
        assert_eq!(second.kind, EventKind::AbilityUsed { slot: 2 });
        assert!(lines.next().is_none());
    }
}
