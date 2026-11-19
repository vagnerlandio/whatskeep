use std::io::Read;

use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BackupInfo {
    version: &'static str,
    size_bytes: u64,
}

/// Identifica o formato (crypt14/crypt15) lendo só o header. Não precisa da chave
/// e não descriptografa nada.
#[tauri::command]
fn inspect_backup(path: String) -> Result<BackupInfo, String> {
    let file = std::fs::File::open(&path).map_err(|e| format!("não foi possível abrir o arquivo: {e}"))?;
    let size_bytes = file.metadata().map_err(|e| e.to_string())?.len();
    let mut head = Vec::with_capacity(4096);
    file.take(4096)
        .read_to_end(&mut head)
        .map_err(|e| e.to_string())?;
    let prefix = wa_crypt::parse_prefix(&head).map_err(|e| e.to_string())?;
    Ok(BackupInfo {
        version: match prefix.version {
            wa_crypt::CryptVersion::Crypt14 => "crypt14",
            wa_crypt::CryptVersion::Crypt15 => "crypt15",
        },
        size_bytes,
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![inspect_backup])
        .run(tauri::generate_context!())
        .expect("falha ao iniciar o aplicativo");
}
