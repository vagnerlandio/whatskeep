mod commands;
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

/// Sem core dump: um crash gravaria a memória do processo (chaves, mensagens
/// decifradas) em disco.
fn disable_core_dumps() {
    #[cfg(unix)]
    {
        let zero = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
        // SAFETY: estrutura válida, chamada sem efeitos além do limite do processo.
        unsafe {
            libc::setrlimit(libc::RLIMIT_CORE, &zero);
        }
    }
}

/// A janela principal só navega dentro do próprio app. Nada de páginas remotas
/// com acesso ao IPC, mesmo que algo tente redirecioná-la. (A janela de login
/// do Google tem a sua própria lista, em gdrive.rs.)
fn main_window_navigation_allowed(url: &tauri::Url) -> bool {
    match url.scheme() {
        "tauri" | "about" => true,
        // Windows serve o app em http(s)://tauri.localhost.
        "http" | "https" if url.host_str() == Some("tauri.localhost") => true,
        // Servidor do Vite, só em desenvolvimento.
        "http" if cfg!(debug_assertions) && url.host_str() == Some("localhost") && url.port() == Some(1420) => true,
        _ => false,
    }
}

/// Contexto do app (config, ACL, assets). Uma expansão só do macro, usada pelo
/// app e pelos testes de ACL.
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
                        tracing::warn!(esquema = url.scheme(), host = url.host_str().unwrap_or(""), "navegação da janela principal BLOQUEADA");
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
            tracing::info!(cofre_existe = wa_vault::Vault::exists(&vault_dir), "app iniciado");
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
            export::export_pick_destination,
            export::export_html,
            export::export_cancel,
            links::open_link,
            diag::ui_log,
            settings::get_language,
            settings::set_language,
        ])
        .run(context())
        .expect("falha ao iniciar o aplicativo");
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

    /// As capabilities reais (capabilities/default.json + manifesto do build.rs):
    /// só a janela principal, com a origem do app, chama comandos.
    #[test]
    fn acl_so_a_janela_principal_chama_comandos() {
        let app = mock_builder()
            .manage(crate::state::AppState::new(std::path::PathBuf::from("/nao/existe")))
            .invoke_handler(tauri::generate_handler![crate::commands::vault_status])
            .build(crate::context())
            .unwrap();
        let main = WebviewWindowBuilder::new(&app, "main", WebviewUrl::default()).build().unwrap();
        let login = WebviewWindowBuilder::new(&app, "google-login", WebviewUrl::default()).build().unwrap();

        let ok = get_ipc_response(&main, request("vault_status", "tauri://localhost"));
        assert!(ok.is_ok(), "{ok:?}");
        let body = ok.unwrap().deserialize::<serde_json::Value>().unwrap();
        assert_eq!(body["state"], "none");

        // Janela sem capability (a do login do Google): recusada.
        assert!(get_ipc_response(&login, request("vault_status", "tauri://localhost")).is_err());
        // Página remota dentro da janela principal: recusada.
        assert!(get_ipc_response(&main, request("vault_status", "https://evil.example/")).is_err());
        // Comando que não existe no manifesto: recusado.
        assert!(get_ipc_response(&main, request("plugin:webview|internal_toggle_devtools", "tauri://localhost")).is_err());
    }

    #[test]
    fn navegacao_da_janela_principal() {
        let ok = |u: &str| super::main_window_navigation_allowed(&u.parse().unwrap());
        assert!(ok("tauri://localhost/index.html"));
        assert!(ok("http://tauri.localhost/"));
        assert!(ok("about:blank"));
        assert!(!ok("https://evil.example/"));
        assert!(!ok("file:///Users/x/pagina.html"));
        assert!(!ok("http://localhost:8080/"));
        assert!(!ok("wamedia://localhost/media/whatsapp/1"));
    }
}
