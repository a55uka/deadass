mod commands;

use commands::Backend;
use deadass_companion::pipeline;
use std::sync::Arc;
use tauri::Manager;

fn main() {
    init_logging();
    tauri::Builder::default()
        .setup(|app| {
            let backend: Backend = Arc::new(tauri::async_runtime::block_on(async {
                pipeline::start(pipeline::load_config())
            }));
            app.manage(backend);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_status,
            commands::set_data_source,
            commands::set_trigger,
            commands::set_config,
            commands::connect_embedded,
            commands::connect_central,
            commands::disconnect,
            commands::rescan,
            commands::test_fire,
            commands::test_openshock,
            commands::test_toys,
            commands::check_updates,
            commands::update_offsets_now,
            commands::update_app_now,
        ])
        .run(tauri::generate_context!())
        .expect("deadass desktop failed to run");
}

fn init_logging() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
}
