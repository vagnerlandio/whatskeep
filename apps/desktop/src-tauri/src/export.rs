//! Encrypted HTML export (crate `wa-export`), triggered by the UI.
//!
//! The destination folder is chosen in the native picker by the backend and
//! kept here: the webview never supplies a path to write to.

use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use wa_export::{Group, MediaMode, Options, Summary};
use zeroize::Zeroizing;

use crate::commands::{blocking, source_app};
use crate::error::CmdError;
use crate::state::AppState;

const SHELL: &str = include_str!(concat!(env!("OUT_DIR"), "/export-shell.html"));
const EVENT_EXPORT: &str = "export-progress";

#[derive(Default)]
pub struct ExportState {
    dest: Mutex<Option<PathBuf>>,
    pub cancel: AtomicBool,
}

impl ExportState {
    fn dest(&self) -> std::sync::MutexGuard<'_, Option<PathBuf>> {
        self.dest.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl From<wa_export::Error> for CmdError {
    fn from(e: wa_export::Error) -> Self {
        use wa_export::Error as E;
        let code = match &e {
            E::Invalid(_) => "invalid_argument",
            E::DestinationNotEmpty => "destination_not_empty",
            E::Cancelled => "cancelled",
            E::Io(_) => "io",
            _ => "internal",
        };
        CmdError::new(code, e.to_string())
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Destination {
    /// Full path, for the user to check.
    path: String,
    /// Inside a cloud-synced folder (iCloud, Dropbox...).
    cloud: Option<&'static str>,
}

/// Synced folders: plaintext media there ends up in the cloud.
fn cloud_provider(path: &std::path::Path) -> Option<&'static str> {
    let p = path.to_string_lossy().to_lowercase();
    [
        ("mobile documents", "iCloud Drive"),
        ("icloud", "iCloud Drive"),
        ("dropbox", "Dropbox"),
        ("onedrive", "OneDrive"),
        ("google drive", "Google Drive"),
        ("googledrive", "Google Drive"),
        ("library/cloudstorage", "um serviço de nuvem"),
    ]
    .into_iter()
    .find(|(k, _)| p.contains(k))
    .map(|(_, v)| v)
}

/// Picks where to save. Creates a new `historico-whatsapp-<date>` folder inside it.
#[tauri::command]
pub async fn export_pick_destination(app: AppHandle) -> Result<Option<Destination>, CmdError> {
    use tauri_plugin_dialog::DialogExt;
    blocking("export_pick_destination", move || {
        let Some(folder) = app.dialog().file().set_title(crate::settings::tr(&app, "Onde salvar o histórico exportado", "Where to save the exported history")).blocking_pick_folder() else {
            return Ok(None);
        };
        let base = folder.into_path().map_err(|_| CmdError::new("invalid_argument", "pasta inválida"))?;
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let mut dest = base.join(format!("historico-whatsapp-{}", date_label(stamp)));
        let mut n = 2;
        while dest.exists() {
            dest = base.join(format!("historico-whatsapp-{}-{n}", date_label(stamp)));
            n += 1;
        }
        let out = Destination { path: dest.display().to_string(), cloud: cloud_provider(&dest) };
        *app.state::<AppState>().export.dest() = Some(dest);
        Ok(Some(out))
    })
    .await
}

/// `YYYY-MM-DD` (UTC) without depending on a date crate.
fn date_label(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    // civil_from_days algorithm (Howard Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

#[derive(Deserialize)]
pub struct GroupInput {
    password: String,
    chats: Vec<i64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    summary: Summary,
    path: String,
}

#[tauri::command]
pub async fn export_html(
    app: AppHandle,
    store: String,
    groups: Vec<GroupInput>,
    media: MediaMode,
) -> Result<ExportResult, CmdError> {
    let groups: Vec<Group> = groups
        .into_iter()
        .map(|g| Group { password: Zeroizing::new(g.password), chats: g.chats })
        .collect();
    let target = source_app(&store)?;
    blocking("export_html", move || {
        if SHELL.is_empty() {
            return Err(CmdError::new("internal", "o visualizador do export não foi incluído nesta versão do app"));
        }
        let state = app.state::<AppState>();
        let _busy = state.busy();
        let dest = state
            .export
            .dest()
            .take()
            .ok_or_else(|| CmdError::new("invalid_argument", "escolha onde salvar"))?;
        state.export.cancel.store(false, Ordering::SeqCst);
        let opts = Options { groups, media, kdf: wa_export::format::Kdf::DEFAULT };

        // Its own connections (not the viewer's): the export may take minutes.
        let (store_handle, media_handle) = {
            let vault = state.vault();
            let vault = vault.as_ref().ok_or_else(CmdError::locked)?;
            (vault.store(target)?, vault.media(target)?)
        };
        let conn = store_handle
            .open_read_only()?
            .ok_or_else(|| CmdError::new("no_store", "nenhum histórico importado deste app"))?;
        let reader = if media == MediaMode::None { None } else { media_handle.reader()? };

        tracing::info!(
            app = target.as_str(),
            passwords = opts.groups.len(),
            chats = opts.groups.iter().map(|g| g.chats.len()).sum::<usize>(),
            media = ?media,
            "export: starting"
        );
        let t = crate::diag::timer("export");
        let result = wa_export::export(&conn, reader.as_ref(), SHELL, &dest, &opts, &state.export.cancel, |p| {
            let _ = app.emit(EVENT_EXPORT, p.clone());
        });
        match result {
            Ok(summary) => {
                tracing::info!(ms = t.ms() as u64, summary = ?summary, "export: done");
                Ok(ExportResult { summary, path: dest.display().to_string() })
            }
            Err(e) => {
                // A half-written package is useless: delete the folder the app created.
                let _ = std::fs::remove_dir_all(&dest);
                Err(e.into())
            }
        }
    })
    .await
}

#[tauri::command]
pub fn export_cancel(app: AppHandle) {
    app.state::<AppState>().export.cancel.store(true, Ordering::SeqCst);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn date() {
        assert_eq!(date_label(0), "1970-01-01");
        assert_eq!(date_label(1_795_000_000), "2026-11-18");
        assert_eq!(date_label(951_782_400), "2000-02-29");
    }

    #[test]
    fn cloud() {
        let p = |s: &str| cloud_provider(std::path::Path::new(s));
        assert_eq!(p("/Users/x/Library/Mobile Documents/com~apple~CloudDocs/h"), Some("iCloud Drive"));
        assert_eq!(p("/Users/x/Library/CloudStorage/Dropbox/h"), Some("Dropbox"));
        assert_eq!(p("/Users/x/Documents/h"), None);
    }
}
