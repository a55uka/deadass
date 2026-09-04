use crate::haptics::HapticCommand;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionMode {
    Disconnected,
    ExternalCentral,
    Embedded,
}

#[derive(Debug, Clone)]
pub struct ToyDevice {
    pub id: String,
    pub name: String,
    pub can_vibrate: bool,
}

impl ToyDevice {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            can_vibrate: true,
        }
    }
}

pub struct ToyHub {
    mode: ConnectionMode,
    central_url: String,
    devices: Vec<ToyDevice>,
}

impl ToyHub {
    pub fn new(central_url: String) -> Self {
        Self {
            mode: ConnectionMode::Disconnected,
            central_url,
            devices: Vec::new(),
        }
    }

    pub fn mode(&self) -> ConnectionMode {
        self.mode
    }

    pub fn central_url(&self) -> &str {
        &self.central_url
    }

    pub fn devices(&self) -> &[ToyDevice] {
        &self.devices
    }

    pub fn use_external(&mut self) {
        self.mode = ConnectionMode::ExternalCentral;
    }

    pub fn use_embedded(&mut self) {
        self.mode = ConnectionMode::Embedded;
    }

    pub fn disconnect(&mut self) {
        self.mode = ConnectionMode::Disconnected;
        self.devices.clear();
    }

    pub fn note_devices(&mut self, devices: Vec<ToyDevice>) {
        self.devices = devices;
    }

    pub async fn play(&self, _command: HapticCommand) {}
}
