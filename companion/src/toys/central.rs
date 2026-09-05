use std::time::Duration;

use buttplug_client::ButtplugClient;
use buttplug_client::connector::ButtplugRemoteClientConnector;
use buttplug_client::serializer::ButtplugClientJSONSerializer;
use buttplug_transport_websocket_tungstenite::ButtplugWebsocketClientTransport;

use super::{ToyError, backend::ButtplugToyBackend};

pub async fn connect_central(url: &str) -> Result<ButtplugToyBackend, ToyError> {
    let connector = ButtplugRemoteClientConnector::<
        ButtplugWebsocketClientTransport,
        ButtplugClientJSONSerializer,
    >::new(ButtplugWebsocketClientTransport::new_insecure_connector(
        url.trim(),
    ));
    let client = ButtplugClient::new("Deadass Companion");
    client
        .connect(connector)
        .await
        .map_err(|error| ToyError::Buttplug(error.to_string()))?;
    settle_device_list(&client).await;
    Ok(ButtplugToyBackend::new(client))
}

async fn settle_device_list(client: &ButtplugClient) {
    for _ in 0..20 {
        if !client.devices().is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}
