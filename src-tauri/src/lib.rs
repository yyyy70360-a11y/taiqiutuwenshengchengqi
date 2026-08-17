mod ai;
mod commands;
mod models;
mod render;
mod storage;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
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
