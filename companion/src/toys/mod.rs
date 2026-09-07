mod backend;
mod central;
mod embedded;

pub use backend::ButtplugToyBackend;

use crate::haptics::HapticCommand;
use thiserror::Error;

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
    pub fn vibrating(id: impl Into<String>, name: impl Into<String>, can_vibrate: bool) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            can_vibrate,
        }
    }

    pub fn names(devices: &[Self]) -> Vec<String> {
        devices.iter().map(|device| device.name.clone()).collect()
    }
}

#[derive(Debug, Error)]
pub enum ToyError {
    #[error("{0}")]
    Buttplug(String),
    #[error("toy backend is not connected")]
    NotConnected,
}

pub struct ToyHub {
    mode: ConnectionMode,
    central_url: String,
    backend: Option<ButtplugToyBackend>,
}

impl ToyHub {
    pub fn new(central_url: String) -> Self {
        Self {
            mode: ConnectionMode::Disconnected,
            central_url,
            backend: None,
        }
    }

    pub fn mode(&self) -> ConnectionMode {
        self.mode
    }

    pub fn devices(&self) -> Vec<ToyDevice> {
        self.backend
            .as_ref()
            .map(ButtplugToyBackend::devices)
            .unwrap_or_default()
    }

    pub async fn connect_embedded(&mut self) -> Result<(), ToyError> {
        self.backend = Some(embedded::connect_embedded().await?);
        self.mode = ConnectionMode::Embedded;
        Ok(())
    }

    pub async fn connect_central(&mut self) -> Result<(), ToyError> {
        self.backend = Some(central::connect_central(&self.central_url).await?);
        self.mode = ConnectionMode::ExternalCentral;
        Ok(())
    }

    pub async fn rescan(&self) -> Result<(), ToyError> {
        match &self.backend {
            Some(backend) => backend.rescan().await,
            None => Err(ToyError::NotConnected),
        }
    }

    pub async fn play(&self, command: HapticCommand) {
        if let Some(backend) = &self.backend {
            backend.play(command).await;
        }
    }

    pub async fn disconnect(&mut self) {
        if let Some(backend) = self.backend.take() {
            backend.disconnect().await;
        }
        self.mode = ConnectionMode::Disconnected;
    }
}
