use buttplug_client::{
    ButtplugClient, ButtplugClientDevice,
    device::{ClientDeviceCommandValue, ClientDeviceOutputCommand},
};
use buttplug_core::message::OutputType;

use super::{ToyDevice, ToyError};
use crate::haptics::HapticCommand;
use deadass_shared::Pattern;

const PULSE_COUNT: u64 = 3;
const RAMP_STEPS: [f64; 3] = [0.4, 0.7, 1.0];

pub struct ButtplugToyBackend {
    client: ButtplugClient,
}

impl ButtplugToyBackend {
    pub fn new(client: ButtplugClient) -> Self {
        Self { client }
    }

    pub fn devices(&self) -> Vec<ToyDevice> {
        self.vibrating_devices()
            .into_iter()
            .map(|device| {
                ToyDevice::vibrating(
                    device.index().to_string(),
                    device.name().to_string(),
                    device.output_available(OutputType::Vibrate),
                )
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
        let targets = self.vibrating_devices();
        let strength = command.strength.clamp(0.0, 1.0);
        match command.pattern {
            Pattern::Vibrate => self.hold(&targets, strength, command.duration_ms).await,
            Pattern::Pulse => self.pulse(&targets, strength, command.duration_ms).await,
            Pattern::Ramp => self.ramp(&targets, strength, command.duration_ms).await,
        }
    }

    pub async fn disconnect(self) {
        let _ = self.client.disconnect().await;
    }

    fn vibrating_devices(&self) -> Vec<ButtplugClientDevice> {
        self.client
            .devices()
            .into_values()
            .filter(|device| device.output_available(OutputType::Vibrate))
            .collect()
    }

    async fn hold(&self, targets: &[ButtplugClientDevice], strength: f64, duration_ms: u64) {
        self.set_vibration(targets, strength).await;
        if duration_ms > 0 {
            sleep_ms(duration_ms).await;
            self.set_vibration(targets, 0.0).await;
        }
    }

    async fn pulse(&self, targets: &[ButtplugClientDevice], strength: f64, duration_ms: u64) {
        let each = duration_ms / (PULSE_COUNT * 2).max(1);
        for _ in 0..PULSE_COUNT {
            self.set_vibration(targets, strength).await;
            sleep_ms(each).await;
            self.set_vibration(targets, 0.0).await;
            sleep_ms(each).await;
        }
    }

    async fn ramp(&self, targets: &[ButtplugClientDevice], strength: f64, duration_ms: u64) {
        let each = duration_ms / RAMP_STEPS.len() as u64;
        for step in RAMP_STEPS {
            self.set_vibration(targets, strength * step).await;
            sleep_ms(each).await;
        }
        self.set_vibration(targets, 0.0).await;
    }

    async fn set_vibration(&self, targets: &[ButtplugClientDevice], strength: f64) {
        for device in targets {
            let _ = device
                .run_output(&ClientDeviceOutputCommand::Vibrate(
                    ClientDeviceCommandValue::Percent(strength),
                ))
                .await;
        }
    }
}

async fn sleep_ms(duration_ms: u64) {
    tokio::time::sleep(std::time::Duration::from_millis(duration_ms)).await;
}
