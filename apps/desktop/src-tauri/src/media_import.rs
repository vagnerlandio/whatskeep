//! Media import into the vault: from the folder copied from the phone or
//! from Google Drive.
//!
//! Only files used by some message in the history get in (by their path on
//! the phone). Deleted messages do not count. Status photos, wallpapers and
//! other files in the folder are left out.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use wa_import::SourceApp;
use wa_vault::media::normalize_path;
use wa_vault::query::{self, MediaRef};
use wa_vault::{MediaWriter, Put};

use crate::commands::{blocking, source_app};
use crate::error::CmdError;
use crate::state::AppState;

const EVENT_MEDIA: &str = "media-progress";

/// What the history expects, indexed by normalized path (`Media/...`).
pub(crate) struct Plan {
    wanted: HashMap<String, MediaRef>,
}

impl Plan {
    pub fn load(state: &AppState, app: SourceApp) -> Result<Self, CmdError> {
        let refs = state.with_store(app, |c| Ok(query::media_refs(c)?))?;
        let total = refs.len();
        let without_path = refs.iter().filter(|r| r.file_path.is_none()).count();
        let with_hash = refs.iter().filter(|r| r.file_hash.is_some()).count();
        let wanted: HashMap<String, MediaRef> = refs
            .into_iter()
            .filter_map(|r| Some((normalize_path(r.file_path.as_deref()?)?, r)))
            .collect();
        let mut by_folder: std::collections::BTreeMap<&str, u64> = Default::default();
        for k in wanted.keys() {
            *by_folder.entry(media_folder(k)).or_default() += 1;
        }
        tracing::info!(
            references = total,
            without_path = without_path,
            with_file_hash = with_hash,
            unique_paths = wanted.len(),
            by_folder = ?by_folder,
            "media used by the history"
        );
        Ok(Self { wanted })
    }

    pub fn get(&self, path: &str) -> Option<&MediaRef> {
        self.wanted.get(&normalize_path(path)?)
    }

    pub fn len(&self) -> usize {
        self.wanted.len()
    }
}

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaSummary {
    /// History files found in the source.
    pub matched: u64,
    pub added: u64,
    /// Were already in the vault.
    pub already: u64,
    /// In the source, but with no message using them.
    pub unmatched: u64,
    /// Failed (read, integrity). The others carry on.
    pub failed: u64,
    pub bytes: u64,
    /// Media in the history (with a known path).
    pub expected: u64,
    /// Why it stopped before the end, if it did. The frontend matches these
    /// (Portuguese) strings by exact value.
    pub stopped: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct MediaProgress {
    done: u64,
    total: u64,
    bytes: u64,
    total_bytes: u64,
    added: u64,
}

/// Emits progress at most every 150 ms.
pub(crate) struct Progress<'a> {
    app: &'a AppHandle,
    last: Instant,
    pub total: u64,
    pub total_bytes: u64,
}

impl<'a> Progress<'a> {
    pub fn new(app: &'a AppHandle, total: u64, total_bytes: u64) -> Self {
        Self { app, last: Instant::now() - Duration::from_secs(1), total, total_bytes }
    }

    pub fn report(&mut self, s: &MediaSummary, force: bool) {
        if !force && self.last.elapsed() < Duration::from_millis(150) {
            return;
        }
        self.last = Instant::now();
        let _ = self.app.emit(
            EVENT_MEDIA,
            MediaProgress {
                done: s.added + s.already + s.failed,
                total: self.total,
                bytes: s.bytes,
                total_bytes: self.total_bytes,
                added: s.added,
            },
        );
    }
}

/// Stores content in the vault, counting the outcome in the summary.
pub(crate) fn store_one(
    writer: &MediaWriter,
    summary: &mut MediaSummary,
    tally: &mut crate::diag::Tally,
    want: &MediaRef,
    data: impl std::io::Read,
    expected: Option<&str>,
) {
    match writer.put(data, expected, want.mime.as_deref(), want.file_path.as_deref()) {
        Ok((_, Put::Added)) => summary.added += 1,
        Ok((_, Put::AlreadyStored)) => summary.already += 1,
        Err(e) => {
            tally.add("store in vault", format!("{e} (kind {})", want.kind));
            summary.failed += 1
        }
    }
}

/// Subfolder of `Media/` (e.g. `WhatsApp Images`), for counts in the log.
fn media_folder(normalized: &str) -> &str {
    normalized.strip_prefix("Media/").and_then(|r| r.split('/').next()).unwrap_or("(other)")
}

pub(crate) fn already_stored(writer: &MediaWriter, want: &MediaRef) -> bool {
    writer.find(want.file_hash.as_deref(), want.file_path.as_deref()).ok().flatten().is_some()
}

// ---------------------------------------------------------------- local folder

/// Regular files under `root`, without following symlinks and without going
/// into hidden folders (`.Statuses`, `.Shared`...).
fn walk(root: &Path, out: &mut Vec<PathBuf>, cancel: &dyn Fn() -> bool) {
    let Ok(entries) = std::fs::read_dir(root) else { return };
    for e in entries.flatten() {
        if cancel() {
            return;
        }
        if e.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        let Ok(ft) = e.file_type() else { continue };
        if ft.is_dir() {
            walk(&e.path(), out, cancel);
        } else if ft.is_file() {
            out.push(e.path());
        }
    }
}

/// Asks for the WhatsApp folder copied from the phone and imports its media.
#[tauri::command]
pub async fn import_media_folder(app: AppHandle, source_app: String) -> Result<Option<MediaSummary>, CmdError> {
    use tauri_plugin_dialog::DialogExt;

    let target = self::source_app(&source_app)?;
    blocking("import_media_folder", move || {
        // The native picker runs in the backend: the webview does not choose paths.
        let Some(folder) = app
            .dialog()
            .file()
            .set_title(crate::settings::tr(&app, "Escolha a pasta WhatsApp copiada do celular", "Choose the WhatsApp folder copied from the phone"))
            .blocking_pick_folder()
        else {
            return Ok(None);
        };
        let root = folder.into_path().map_err(|_| CmdError::new("invalid_argument", "pasta inválida"))?;
        import_folder(&app, target, &root).map(Some)
    })
    .await
}

fn import_folder(app: &AppHandle, target: SourceApp, root: &Path) -> Result<MediaSummary, CmdError> {
    let state = app.state::<AppState>();
    let _busy = state.busy();
    state.media_cancel.store(false, Ordering::SeqCst);
    let plan = Plan::load(&state, target)?;
    let handle = state.vault().as_ref().ok_or_else(CmdError::locked)?.media(target)?;
    let writer = handle.writer()?;
    writer.cleanup();
    let cancelled = || state.media_cancel.load(Ordering::SeqCst);

    let mut files = Vec::new();
    let t = crate::diag::timer("list the media folder");
    walk(root, &mut files, &cancelled);
    drop(t);
    let mut summary = MediaSummary { expected: plan.len() as u64, ..Default::default() };
    let mut tally = crate::diag::Tally::new("folder media");
    let mut todo = Vec::new();
    let mut unmatched_by_folder: std::collections::BTreeMap<String, u64> = Default::default();
    let mut outside_media = 0u64;
    for path in files.iter() {
        let rel = path.to_string_lossy();
        match plan.get(&rel) {
            Some(want) => todo.push((path.clone(), want)),
            None => {
                summary.unmatched += 1;
                match normalize_path(&rel) {
                    Some(n) => *unmatched_by_folder.entry(media_folder(&n).to_owned()).or_default() += 1,
                    None => outside_media += 1,
                }
            }
        }
    }
    summary.matched = todo.len() as u64;
    tracing::info!(
        files_in_folder = files.len(),
        expected = plan.len(),
        matched = summary.matched,
        outside_media = outside_media,
        unmatched_by_folder = ?unmatched_by_folder,
        "media folder scanned"
    );
    if !files.is_empty() && outside_media == files.len() as u64 {
        tracing::warn!("no file is under a Media/ folder: the chosen folder is probably not the WhatsApp one");
    }
    let total_bytes = todo.iter().filter_map(|(p, _)| p.metadata().ok()).map(|m| m.len()).sum();
    let mut progress = Progress::new(app, todo.len() as u64, total_bytes);

    for (path, want) in todo {
        if cancelled() {
            summary.stopped = Some("cancelado".into());
            break;
        }
        let size = path.metadata().map(|m| m.len()).unwrap_or(0);
        if already_stored(&writer, want) {
            summary.already += 1;
        } else {
            match std::fs::File::open(&path) {
                // Local file: the content is whatever the user has. Does not
                // require WhatsApp's hash (the file may have been recompressed)
                // and stays findable by path.
                Ok(f) => store_one(&writer, &mut summary, &mut tally, want, std::io::BufReader::new(f), None),
                Err(e) => {
                    tally.add("open file", e);
                    summary.failed += 1
                }
            }
        }
        summary.bytes += size;
        progress.report(&summary, false);
    }
    progress.report(&summary, true);
    tally.report();
    tracing::info!(
        added = summary.added,
        already_in_vault = summary.already,
        failed = summary.failed,
        stopped = ?summary.stopped,
        "media folder: done"
    );
    Ok(summary)
}

/// Stops the media import in progress (what is already in stays).
#[tauri::command]
pub fn media_cancel(app: AppHandle) {
    app.state::<AppState>().media_cancel.store(true, Ordering::SeqCst);
}

// ---------------------------------------------------------------- export

/// Saves a decrypted media file wherever the user chooses (an explicit
/// action: the file leaves the vault in plaintext).
#[tauri::command]
pub async fn save_media(app: AppHandle, store: String, message_id: i64) -> Result<bool, CmdError> {
    use tauri_plugin_dialog::DialogExt;

    let target = self::source_app(&store)?;
    blocking("save_media", move || {
        let state = app.state::<AppState>();
        let r = state
            .with_store(target, |c| Ok(query::media_ref(c, message_id)?))?
            .ok_or_else(|| CmdError::new("not_found", "mídia não encontrada"))?;
        let blob = state
            .with_media(target, |m| Ok(m.and_then(|m| m.find(r.file_hash.as_deref(), r.file_path.as_deref()).ok().flatten())))?
            .ok_or_else(|| CmdError::new("not_found", "o arquivo desta mídia não está no cofre"))?;
        let suggested = suggested_name(&r);
        let Some(dest) = app.dialog().file().set_file_name(&suggested).blocking_save_file() else {
            return Ok(false);
        };
        let dest = dest.into_path().map_err(|_| CmdError::new("invalid_argument", "destino inválido"))?;
        let tmp = dest.with_extension("parcial");
        let result = state.with_media(target, |m| {
            let m = m.ok_or_else(|| CmdError::new("not_found", "mídia não encontrada"))?;
            let f = std::fs::File::create(&tmp)?;
            m.copy_to(&blob, std::io::BufWriter::new(f))?;
            Ok(())
        });
        match result {
            Ok(()) => {
                std::fs::rename(&tmp, &dest)?;
                Ok(true)
            }
            Err(e) => {
                let _ = std::fs::remove_file(&tmp);
                Err(e)
            }
        }
    })
    .await
}

/// Suggested name: the document's original name, or the file's name on the
/// phone. No folder separators (the name comes from the backup, untrusted).
fn suggested_name(r: &MediaRef) -> String {
    let raw = r
        .file_name
        .clone()
        .or_else(|| r.file_path.as_deref().and_then(|p| p.rsplit(['/', '\\']).next()).map(str::to_owned))
        .unwrap_or_else(|| format!("midia-{}", r.message_id));
    let clean: String = raw
        .chars()
        .map(|c| if c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') { '_' } else { c })
        .collect();
    let clean = clean.trim_start_matches('.').trim();
    if clean.is_empty() { format!("midia-{}", r.message_id) } else { clean.chars().take(150).collect() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(name: Option<&str>, path: Option<&str>) -> MediaRef {
        MediaRef {
            message_id: 7,
            kind: "document".into(),
            file_hash: None,
            file_path: path.map(Into::into),
            mime: None,
            file_name: name.map(Into::into),
        }
    }

    #[test]
    fn safe_suggested_name() {
        assert_eq!(suggested_name(&r(Some("contract.pdf"), None)), "contract.pdf");
        assert_eq!(suggested_name(&r(Some("../../etc/passwd"), None)), "_.._etc_passwd");
        assert_eq!(suggested_name(&r(None, Some("Media/WhatsApp Documents/a b.pdf"))), "a b.pdf");
        assert_eq!(suggested_name(&r(Some("..."), None)), "midia-7");
        assert_eq!(suggested_name(&r(None, None)), "midia-7");
    }
}
