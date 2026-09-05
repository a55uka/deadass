use buttplug_client::ButtplugClient;
use buttplug_client_in_process::ButtplugInProcessClientConnectorBuilder;
use buttplug_server::{ButtplugServerBuilder, device::ServerDeviceManagerBuilder};
use buttplug_server_device_config::load_protocol_configs;

use super::{ToyError, backend::ButtplugToyBackend};

pub async fn connect_embedded() -> Result<ButtplugToyBackend, ToyError> {
    let device_config = load_protocol_configs(&None, &None, false)
        .map_err(|error| ToyError::Buttplug(error.to_string()))?
        .finish()
        .map_err(|error| ToyError::Buttplug(error.to_string()))?;

    let mut device_manager = ServerDeviceManagerBuilder::new(device_config);
    device_manager.comm_manager(
        buttplug_server_hwmgr_btleplug::BtlePlugCommunicationManagerBuilder::default(),
    );
    device_manager.comm_manager(
        buttplug_server_hwmgr_lovense_connect::LovenseConnectServiceCommunicationManagerBuilder::default(
        ),
    );
    device_manager.comm_manager(
        buttplug_server_hwmgr_lovense_dongle::LovenseHIDDongleCommunicationManagerBuilder::default(
        ),
    );
    device_manager.comm_manager(
        buttplug_server_hwmgr_websocket::WebsocketServerDeviceCommunicationManagerBuilder::default(
        )
        .listen_on_all_interfaces(true),
    );
    #[cfg(target_os = "windows")]
    device_manager.comm_manager(
        buttplug_server_hwmgr_xinput::XInputDeviceCommunicationManagerBuilder::default(),
    );

    let server = ButtplugServerBuilder::new(
        device_manager
            .finish()
            .map_err(|error| ToyError::Buttplug(error.to_string()))?,
    )
    .finish()
    .map_err(|error| ToyError::Buttplug(error.to_string()))?;

    let connector = ButtplugInProcessClientConnectorBuilder::default()
        .server(server)
        .finish();
    let client = ButtplugClient::new("Deadass Companion (Embedded)");
    client
        .connect(connector)
        .await
        .map_err(|error| ToyError::Buttplug(error.to_string()))?;
    client
        .start_scanning()
        .await
        .map_err(|error| ToyError::Buttplug(error.to_string()))?;
    Ok(ButtplugToyBackend::new(client))
}
