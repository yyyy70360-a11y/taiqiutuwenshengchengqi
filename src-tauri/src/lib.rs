mod cloud;
mod commands;
mod models;
mod render;
mod startup;
mod storage;
#[cfg(target_os = "windows")]
mod tunnel;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            startup::begin(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_templates,
            commands::get_presets,
            commands::render_preview,
            commands::render_save,
            commands::render_batch,
            commands::open_output_folder,
            commands::get_settings,
            commands::set_settings,
            commands::get_api_key_status,
            commands::get_cloud_status,
            commands::set_cloud_server_url,
            commands::test_cloud_connection,
            commands::retry_startup,
            commands::continue_startup_local,
            commands::cloud_register,
            commands::cloud_register_application,
            commands::cloud_login,
            commands::cloud_logout,
            commands::cloud_sync_upload,
            commands::cloud_sync_download,
            commands::get_accounts,
            commands::set_accounts,
            commands::get_copy_library,
            commands::save_copy_library,
            commands::migrate_copy_library,
            commands::get_render_history,
            commands::read_history_image,
            commands::clear_render_history,
            commands::migrate_legacy,
            commands::generate_copy,
            commands::generate_batch_copy
        ])
        .run(tauri::generate_context!())
        .expect("error while running billiards matrix");
}
