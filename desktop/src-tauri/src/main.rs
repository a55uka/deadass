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
            commands::toys::get_status,
            commands::config::set_data_source,
            commands::config::set_trigger,
            commands::config::set_config,
            commands::toys::connect_embedded,
            commands::toys::connect_central,
            commands::toys::disconnect,
            commands::toys::rescan,
            commands::toys::test_fire,
            commands::toys::test_openshock,
            commands::toys::test_toys,
            commands::updates::check_updates,
            commands::updates::update_offsets_now,
            commands::updates::update_app_now,
        ])
        .run(tauri::generate_context!())
        .expect("deadass desktop failed to run");
}

fn init_logging() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
}
