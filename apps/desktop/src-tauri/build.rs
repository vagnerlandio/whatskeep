use std::path::Path;

fn main() {
    // O visualizador do export HTML (packages/export-shell) entra no binário.
    // Sem ele compilado, o app funciona, mas o export avisa que está indisponível.
    let shell = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../packages/export-shell/dist/index.html");
    println!("cargo:rerun-if-changed={}", shell.display());
    let out = Path::new(&std::env::var("OUT_DIR").unwrap()).join("export-shell.html");
    match std::fs::read(&shell) {
        Ok(html) => std::fs::write(&out, html).unwrap(),
        Err(_) => {
            println!("cargo:warning=export-shell não compilado: rode `pnpm --filter export-shell build`");
            std::fs::write(&out, b"").unwrap();
        }
    }
    // Comandos declarados explicitamente: só as janelas com a capability
    // correspondente (a principal) podem chamá-los. A janela de login do
    // Google não tem nenhuma.
    let commands: &[&str] = &[
        "vault_status",
        "vault_create",
        "vault_unlock",
        "vault_lock",
        "vault_touch",
        "vault_change_password",
        "list_stores",
        "inspect_backup",
        "import_backup",
        "list_chats",
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
    .expect("falha no build do Tauri");
}
