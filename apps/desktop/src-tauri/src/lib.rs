mod commands;
mod error;
mod state;

use tauri::Manager;

use state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let vault_dir = app.path().app_data_dir()?.join("vault");
            app.manage(AppState::new(vault_dir));
            state::spawn_autolock(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::vault_status,
            commands::vault_create,
            commands::vault_unlock,
            commands::vault_lock,
            commands::vault_touch,
            commands::vault_change_password,
            commands::list_stores,
            commands::inspect_backup,
            commands::import_backup,
        ])
        .run(tauri::generate_context!())
        .expect("falha ao iniciar o aplicativo");
}
