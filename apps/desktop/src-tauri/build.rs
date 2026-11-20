use std::path::Path;

fn main() {
    // The HTML export viewer (packages/export-shell) is embedded in the binary.
    // Without it built, the app works, but the export reports it is unavailable.
    let shell = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../packages/export-shell/dist/index.html");
    println!("cargo:rerun-if-changed={}", shell.display());
    let out = Path::new(&std::env::var("OUT_DIR").unwrap()).join("export-shell.html");
    match std::fs::read(&shell) {
        Ok(html) => std::fs::write(&out, html).unwrap(),
        Err(_) => {
            println!("cargo:warning=export-shell not built: run `pnpm --filter export-shell build`");
            std::fs::write(&out, b"").unwrap();
        }
    }
    // Commands declared explicitly: only windows with the matching capability
    // (the main one) can call them. The Google login window has none.
    let commands: &[&str] = &[
        "vault_status",
        "vault_create",
        "vault_unlock",
        "vault_lock",
        "vault_touch",
        "vault_change_password",
        "vault_usage",
        "vault_delete",
        "store_delete",
        "contacts_info",
        "contacts_import",
        "contacts_remove",
        "list_stores",
        "inspect_backup",
        "import_backup",
        "list_chats",
        "set_chat_pinned",
        "get_messages",
        "get_messages_around",
        "search_messages",
        "gdrive_login",
        "gdrive_login_manual",
        "gdrive_cancel",
        "gdrive_download",
        "import_media_folder",
        "media_cancel",
        "save_media",
        "export_pick_destination",
        "export_html",
        "export_cancel",
        "open_link",
        "ui_log",
        "get_language",
        "set_language",
    ];
    tauri_build::try_build(
        tauri_build::Attributes::new().app_manifest(tauri_build::AppManifest::new().commands(commands)),
    )
    .expect("Tauri build failed");
}
