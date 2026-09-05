use buttplug_client::{
    ButtplugClient, ButtplugClientDevice,
    device::{ClientDeviceCommandValue, ClientDeviceOutputCommand},
};
use buttplug_core::message::OutputType;

use super::{ToyDevice, ToyError};
use crate::haptics::HapticCommand;
use deadass_shared::Pattern;

pub struct ButtplugToyBackend {
    client: ButtplugClient,
}

impl ButtplugToyBackend {
    pub fn new(client: ButtplugClient) -> Self {
        Self { client }
    }

    pub fn devices(&self) -> Vec<ToyDevice> {
        self.client
            .devices()
            .into_values()
            .map(|device: ButtplugClientDevice| {
                ToyDevice {
                    id: device.index().to_string(),
                    name: device.name().to_string(),
                    can_vibrate: device.output_available(OutputType::Vibrate),
                }
            })
            .collect()
    }

    pub async fn rescan(&self) -> Result<(), ToyError> {
        self.client
            .start_scanning()
            .await
            .map_err(|error| ToyError::Buttplug(error.to_string()))
    }

    pub async fn play(&self, command: HapticCommand) {
        let strength = command.strength.clamp(0.0, 1.0);
        let targets: Vec<ButtplugClientDevice> = self
            .client
            .devices()
            .into_values()
            .filter(|device| device.output_available(OutputType::Vibrate))
            .collect();
        match command.pattern {
            Pattern::Vibrate => {
                self.vibrate_hold(&targets, strength, command.duration_ms)
                    .await
            }
            Pattern::Pulse => self.vibrate_pulsed(&targets, strength, command.duration_ms).await,
            Pattern::Ramp => self.vibrate_ramp(&targets, strength, command.duration_ms).await,
        }
    }

    pub async fn disconnect(self) {
        let _ = self.client.disconnect().await;
    }

    async fn vibrate_hold(&self, targets: &[ButtplugClientDevice], strength: f64, duration_ms: u64) {
        self.set_all(targets, strength).await;
        if duration_ms > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(duration_ms)).await;
            self.set_all(targets, 0.0).await;
        }
    }

    async fn vibrate_pulsed(
        &self,
        targets: &[ButtplugClientDevice],
        strength: f64,
        duration_ms: u64,
    ) {
        let pulses = 3u64;
        let each = duration_ms / (pulses * 2).max(1);
        for _ in 0..pulses {
            self.set_all(targets, strength).await;
            tokio::time::sleep(std::time::Duration::from_millis(each)).await;
            self.set_all(targets, 0.0).await;
            tokio::time::sleep(std::time::Duration::from_millis(each)).await;
        }
    }

    async fn vibrate_ramp(&self, targets: &[ButtplugClientDevice], strength: f64, duration_ms: u64) {
        let steps = [0.4, 0.7, 1.0];
        let each = duration_ms / steps.len() as u64;
        for step in steps {
            self.set_all(targets, strength * step).await;
            tokio::time::sleep(std::time::Duration::from_millis(each)).await;
        }
        self.set_all(targets, 0.0).await;
    }

    async fn set_all(&self, targets: &[ButtplugClientDevice], strength: f64) {
        for device in targets {
            let _ = device
                .run_output(&ClientDeviceOutputCommand::Vibrate(
                    ClientDeviceCommandValue::Percent(strength),
                ))
                .await;
        }
    }
}
