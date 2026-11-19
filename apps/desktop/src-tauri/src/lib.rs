mod commands;
mod error;
mod gdrive;
mod media;
mod media_import;
mod state;

use tauri::Manager;

use state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .register_asynchronous_uri_scheme_protocol(media::SCHEME, |ctx, request, responder| {
            let app = ctx.app_handle().clone();
            let from_main_window = ctx.webview_label() == "main";
            tauri::async_runtime::spawn_blocking(move || {
                responder.respond(media::handle(&app, from_main_window, &request));
            });
        })
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
            commands::list_chats,
            commands::get_messages,
            commands::get_messages_around,
            commands::search_messages,
            gdrive::gdrive_login,
            gdrive::gdrive_login_manual,
            gdrive::gdrive_cancel,
            gdrive::gdrive_download,
            media_import::import_media_folder,
            media_import::media_cancel,
            media_import::save_media,
        ])
        .run(tauri::generate_context!())
        .expect("falha ao iniciar o aplicativo");
}
