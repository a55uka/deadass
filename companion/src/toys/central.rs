use std::time::Duration;

use buttplug_client::ButtplugClient;
use buttplug_client::connector::ButtplugRemoteClientConnector;
use buttplug_client::serializer::ButtplugClientJSONSerializer;
use buttplug_transport_websocket_tungstenite::ButtplugWebsocketClientTransport;

use super::{ToyError, backend::ButtplugToyBackend};

const DEVICE_SETTLE_POLLS: usize = 20;
const DEVICE_SETTLE_POLL: Duration = Duration::from_millis(100);

pub async fn connect_central(url: &str) -> Result<ButtplugToyBackend, ToyError> {
    let transport = ButtplugWebsocketClientTransport::new_insecure_connector(url.trim());
    let connector =
        ButtplugRemoteClientConnector::<_, ButtplugClientJSONSerializer>::new(transport);
    let client = ButtplugClient::new("Deadass Companion");
    client
        .connect(connector)
        .await
        .map_err(|error| ToyError::Buttplug(error.to_string()))?;
    wait_for_devices(&client).await;
    Ok(ButtplugToyBackend::new(client))
}

async fn wait_for_devices(client: &ButtplugClient) {
    for _ in 0..DEVICE_SETTLE_POLLS {
        if !client.devices().is_empty() {
            break;
        }
        tokio::time::sleep(DEVICE_SETTLE_POLL).await;
    }
}
