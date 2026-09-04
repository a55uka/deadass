use deadass_shared::GameEvent;

pub struct EventSender {
    port: u16,
}

impl EventSender {
    pub fn new(port: u16) -> Self {
        Self { port }
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn endpoint(&self) -> String {
        format!("127.0.0.1:{}", self.port)
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
        use std::io::Write;
        use std::net::TcpStream;
        let payload = Self::encode(events);
        if payload.is_empty() {
            return;
        }
        let Ok(mut stream) = TcpStream::connect(self.endpoint()) else {
            return;
        };
        let _ = stream.write_all(&payload);
    }

    #[cfg(not(windows))]
    pub fn push(&self, _events: &[GameEvent]) {}
}
