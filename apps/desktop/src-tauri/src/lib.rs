mod commands;
mod error;
mod media;
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
                responder.respond(media::handle(&app, from_main_window, request.uri()));
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
        ])
        .run(tauri::generate_context!())
        .expect("falha ao iniciar o aplicativo");
}
