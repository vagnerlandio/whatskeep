mod backup_folder;
mod commands;
mod contacts;
mod diag;
mod error;
mod export;
mod gdrive;
mod links;
mod media;
mod media_import;
mod settings;
mod state;

use tauri::Manager;

use state::AppState;

/// No core dumps: a crash would write the process memory (keys, decrypted
/// messages) to disk.
fn disable_core_dumps() {
    #[cfg(unix)]
    {
        let zero = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
        // SAFETY: valid struct; the call has no effect beyond the process limit.
        unsafe {
            libc::setrlimit(libc::RLIMIT_CORE, &zero);
        }
    }
}

/// The main window only navigates within the app itself. No remote pages with
/// IPC access, even if something tries to redirect it. (The Google login
/// window has its own list, in gdrive.rs.)
fn main_window_navigation_allowed(url: &tauri::Url) -> bool {
    match url.scheme() {
        "tauri" | "about" => true,
        // Windows serves the app at http(s)://tauri.localhost.
        "http" | "https" if url.host_str() == Some("tauri.localhost") => true,
        // Vite dev server, development only.
        "http" if cfg!(debug_assertions) && url.host_str() == Some("localhost") && url.port() == Some(1420) => true,
        _ => false,
    }
}

/// App context (config, ACL, assets). A single macro expansion, used by the
/// app and by the ACL tests.
fn context<R: tauri::Runtime>() -> tauri::Context<R> {
    tauri::generate_context!()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    diag::init();
    disable_core_dumps();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(
            tauri::plugin::Builder::<tauri::Wry>::new("nav-guard")
                .on_navigation(|webview, url| {
                    if webview.label() != "main" {
                        return true;
                    }
                    let ok = main_window_navigation_allowed(url);
                    if !ok {
                        tracing::warn!(scheme = url.scheme(), host = url.host_str().unwrap_or(""), "main window navigation BLOCKED");
                    }
                    ok
                })
                .build(),
        )
        .register_asynchronous_uri_scheme_protocol(media::SCHEME, |ctx, request, responder| {
            let app = ctx.app_handle().clone();
            let from_main_window = ctx.webview_label() == "main";
            tauri::async_runtime::spawn_blocking(move || {
                responder.respond(media::handle(&app, from_main_window, &request));
            });
        })
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            app.manage(settings::Settings::load(&data_dir));
            let vault_dir = data_dir.join("vault");
            tracing::info!(vault_exists = wk_vault::Vault::exists(&vault_dir), "app started");
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
            commands::vault_usage,
            commands::vault_delete,
            commands::store_delete,
            contacts::contacts_info,
            contacts::contacts_import,
            contacts::contacts_remove,
            commands::list_stores,
            commands::inspect_backup,
            commands::import_backup,
            commands::list_chats,
            commands::set_chat_pinned,
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
            export::export_pick_destination,
            export::export_html,
            export::export_cancel,
            links::open_link,
            diag::ui_log,
            settings::get_language,
            settings::set_language,
        ])
        .run(context())
        .expect("failed to start the application");
}

#[cfg(test)]
mod tests {
    use tauri::ipc::{CallbackFn, InvokeBody};
    use tauri::test::{INVOKE_KEY, get_ipc_response, mock_builder};
    use tauri::webview::InvokeRequest;
    use tauri::{WebviewUrl, WebviewWindowBuilder};

    fn request(cmd: &str, origin: &str) -> InvokeRequest {
        InvokeRequest {
            cmd: cmd.into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: origin.parse().unwrap(),
            body: InvokeBody::default(),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.to_string(),
        }
    }

    /// The real capabilities (capabilities/default.json + build.rs manifest):
    /// only the main window, with the app origin, calls commands.
    #[test]
    fn acl_only_main_window_calls_commands() {
        let app = mock_builder()
            .manage(crate::state::AppState::new(std::path::PathBuf::from("/does/not/exist")))
            .invoke_handler(tauri::generate_handler![crate::commands::vault_status])
            .build(crate::context())
            .unwrap();
        let main = WebviewWindowBuilder::new(&app, "main", WebviewUrl::default()).build().unwrap();
        let login = WebviewWindowBuilder::new(&app, "google-login", WebviewUrl::default()).build().unwrap();

        let ok = get_ipc_response(&main, request("vault_status", "tauri://localhost"));
        assert!(ok.is_ok(), "{ok:?}");
        let body = ok.unwrap().deserialize::<serde_json::Value>().unwrap();
        assert_eq!(body["state"], "none");

        // Window without a capability (the Google login one): refused.
        assert!(get_ipc_response(&login, request("vault_status", "tauri://localhost")).is_err());
        // Remote page inside the main window: refused.
        assert!(get_ipc_response(&main, request("vault_status", "https://evil.example/")).is_err());
        // Command that is not in the manifest: refused.
        assert!(get_ipc_response(&main, request("plugin:webview|internal_toggle_devtools", "tauri://localhost")).is_err());
    }

    #[test]
    fn main_window_navigation() {
        let ok = |u: &str| super::main_window_navigation_allowed(&u.parse().unwrap());
        assert!(ok("tauri://localhost/index.html"));
        assert!(ok("http://tauri.localhost/"));
        assert!(ok("about:blank"));
        assert!(!ok("https://evil.example/"));
        assert!(!ok("file:///Users/x/page.html"));
        assert!(!ok("http://localhost:8080/"));
        assert!(!ok("wkmedia://localhost/media/whatsapp/1"));
    }
}
