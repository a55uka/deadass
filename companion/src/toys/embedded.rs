use buttplug_client::ButtplugClient;
use buttplug_client_in_process::ButtplugInProcessClientConnectorBuilder;
use buttplug_server::{ButtplugServer, ButtplugServerBuilder, device::ServerDeviceManagerBuilder};
use buttplug_server_device_config::load_protocol_configs;

use super::{ToyError, backend::ButtplugToyBackend};

pub async fn connect_embedded() -> Result<ButtplugToyBackend, ToyError> {
    let server = embedded_server().map_err(backend_error)?;
    let connector = ButtplugInProcessClientConnectorBuilder::default()
        .server(server)
        .finish();
    let client = ButtplugClient::new("Deadass Companion (Embedded)");
    client.connect(connector).await.map_err(backend_error)?;
    client.start_scanning().await.map_err(backend_error)?;
    Ok(ButtplugToyBackend::new(client))
}

fn embedded_server() -> Result<ButtplugServer, String> {
    let configs = load_protocol_configs(&None, &None, false)
        .map_err(|error| error.to_string())?
        .finish()
        .map_err(|error| error.to_string())?;
    let mut manager = ServerDeviceManagerBuilder::new(configs);
    register_comm_managers(&mut manager);
    ButtplugServerBuilder::new(manager.finish().map_err(|error| error.to_string())?)
        .finish()
        .map_err(|error| error.to_string())
}

fn register_comm_managers(manager: &mut ServerDeviceManagerBuilder) {
    manager.comm_manager(
        buttplug_server_hwmgr_btleplug::BtlePlugCommunicationManagerBuilder::default(),
    );
    manager.comm_manager(
        buttplug_server_hwmgr_lovense_connect::LovenseConnectServiceCommunicationManagerBuilder::default(
        ),
    );
    manager.comm_manager(
        buttplug_server_hwmgr_lovense_dongle::LovenseHIDDongleCommunicationManagerBuilder::default(
        ),
    );
    manager.comm_manager(
        buttplug_server_hwmgr_websocket::WebsocketServerDeviceCommunicationManagerBuilder::default(
        )
        .listen_on_all_interfaces(true),
    );
    #[cfg(target_os = "windows")]
    manager.comm_manager(
        buttplug_server_hwmgr_xinput::XInputDeviceCommunicationManagerBuilder::default(),
    );
}

fn backend_error(error: impl ToString) -> ToyError {
    ToyError::Buttplug(error.to_string())
}
