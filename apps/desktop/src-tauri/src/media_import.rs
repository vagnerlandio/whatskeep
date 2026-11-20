//! Media import into the vault from a folder: the one copied from the phone,
//! or a Google Drive backup the app saved (see `backup_folder`).
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
use wk_crypt::BackupKey;
use wk_import::SourceApp;
use wk_vault::media::normalize_path;
use wk_vault::query::{self, MediaRef};
use wk_vault::{MediaWriter, Put};

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
/// Without the key, `.mcrypt1` media (end-to-end encrypted Drive backups) are
/// left out: those come in through the backup import, which has the key.
#[tauri::command]
pub async fn import_media_folder(app: AppHandle, source_app: String) -> Result<Option<MediaSummary>, CmdError> {
    use tauri_plugin_dialog::DialogExt;

    let target = self::source_app(&source_app)?;
    blocking("import_media_folder", move || {
        let state = app.state::<AppState>();
        let _job = state.start_job()?;
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
        import_folder(&app, target, &root, None).map(Some)
    })
    .await
}

/// One file to bring in: where it is, the message that uses it and, for a
/// `.mcrypt1`, the hex name that derives its key.
struct Todo<'a> {
    path: PathBuf,
    want: &'a MediaRef,
    mcrypt1: Option<String>,
}

/// Imports the media under `root` that the history uses. `key` decrypts the
/// `.mcrypt1` files of a Drive backup saved by the app (their real names are
/// in the folder's manifest).
pub(crate) fn import_folder(
    app: &AppHandle,
    target: SourceApp,
    root: &Path,
    key: Option<&BackupKey>,
) -> Result<MediaSummary, CmdError> {
    let state = app.state::<AppState>();
    let _busy = state.busy();
    state.media_cancel.store(false, Ordering::SeqCst);
    let plan = Plan::load(&state, target)?;
    let handle = state.vault().as_ref().ok_or_else(CmdError::locked)?.media(target)?;
    let writer = handle.writer()?;
    writer.cleanup();
    let cancelled = || state.media_cancel.load(Ordering::SeqCst);
    let manifest = crate::backup_folder::Manifest::load(root);

    let mut files = Vec::new();
    let t = crate::diag::timer("list the media folder");
    walk(root, &mut files, &cancelled);
    drop(t);
    let mut summary = MediaSummary { expected: plan.len() as u64, ..Default::default() };
    let mut tally = crate::diag::Tally::new("folder media");
    let mut todo: Vec<Todo> = Vec::new();
    let mut unmatched_by_folder: std::collections::BTreeMap<String, u64> = Default::default();
    let mut outside_media = 0u64;
    let mut mcrypt1_files = 0u64;
    let mut logged_meta = false;
    for path in files.iter() {
        if path.extension().is_some_and(|e| e == "mcrypt1") {
            mcrypt1_files += 1;
            let rel = path.strip_prefix(root).ok().and_then(|p| p.to_str()).map(|p| p.replace('\\', "/"));
            let meta = manifest.as_ref().zip(rel.as_deref()).and_then(|(m, r)| m.metadata(r));
            let resolved = match (key, meta) {
                (None, _) => Err(("mcrypt1 needs the backup key", String::from("import the backup with its key"))),
                (_, None) => Err(("mcrypt1 without metadata", String::from("not in the folder's Drive manifest"))),
                (Some(key), Some(meta)) => mcrypt1_name(key, meta, &mut logged_meta),
            };
            match resolved {
                Ok(name) => match plan.get(&name) {
                    Some(want) => {
                        let hex = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_owned();
                        todo.push(Todo { path: path.clone(), want, mcrypt1: Some(hex) })
                    }
                    None => summary.unmatched += 1,
                },
                Err((reason, detail)) => {
                    tally.add(reason, detail);
                    summary.unmatched += 1;
                }
            }
            continue;
        }
        let rel = path.to_string_lossy();
        match plan.get(&rel) {
            Some(want) => todo.push(Todo { path: path.clone(), want, mcrypt1: None }),
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
        mcrypt1 = mcrypt1_files,
        manifest = manifest.is_some(),
        unmatched_by_folder = ?unmatched_by_folder,
        "media folder scanned"
    );
    if !files.is_empty() && outside_media == files.len() as u64 {
        tracing::warn!("no file is under a Media/ folder: the chosen folder is probably not the WhatsApp one");
    }
    let total_bytes = todo.iter().filter_map(|t| t.path.metadata().ok()).map(|m| m.len()).sum();
    let mut progress = Progress::new(app, todo.len() as u64, total_bytes);
    // (with a valid GCM tag, without a tag but checked against file_hash)
    let mut mcrypt_stats = (0u64, 0u64);

    for Todo { path, want, mcrypt1 } in todo {
        if cancelled() {
            summary.stopped = Some(crate::backup_folder::STOPPED_CANCELLED.into());
            break;
        }
        let size = path.metadata().map(|m| m.len()).unwrap_or(0);
        if already_stored(&writer, want) {
            summary.already += 1;
        } else {
            match (mcrypt1, key) {
                (Some(hex), Some(key)) => match std::fs::read(&path) {
                    Ok(bytes) => store_mcrypt1(&writer, &mut summary, &mut tally, &mut mcrypt_stats, want, key, &hex, bytes),
                    Err(e) => {
                        tally.add("open file", e);
                        summary.failed += 1
                    }
                },
                _ => match std::fs::File::open(&path) {
                    // Local file: the content is whatever the user has. Does not
                    // require WhatsApp's hash (the file may have been recompressed)
                    // and stays findable by path.
                    Ok(f) => store_one(&writer, &mut summary, &mut tally, want, std::io::BufReader::new(f), None),
                    Err(e) => {
                        tally.add("open file", e);
                        summary.failed += 1
                    }
                },
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
        mcrypt1_with_tag = mcrypt_stats.0,
        mcrypt1_checked_by_hash = mcrypt_stats.1,
        stopped = ?summary.stopped,
        "media folder: done"
    );
    Ok(summary)
}

/// Decrypts a `.mcrypt1` and stores it. If the GCM tag does not verify, the
/// content only gets in if its SHA-256 matches the `file_hash` the msgstore
/// keeps for that media.
#[allow(clippy::too_many_arguments)]
pub(crate) fn store_mcrypt1(
    writer: &MediaWriter,
    summary: &mut MediaSummary,
    tally: &mut crate::diag::Tally,
    stats: &mut (u64, u64),
    want: &MediaRef,
    key: &BackupKey,
    hex: &str,
    bytes: Vec<u8>,
) {
    match wk_crypt::mcrypt1::decrypt_media(key, hex, bytes) {
        Ok(m) if m.verified => {
            stats.0 += 1;
            store_one(writer, summary, tally, want, &m.data[..], None)
        }
        // Not authenticated: only with WhatsApp's hash to check against.
        Ok(m) => match want.file_hash.as_deref() {
            Some(h) => {
                stats.1 += 1;
                store_one(writer, summary, tally, want, &m.data[..], Some(h))
            }
            None => {
                tally.add("mcrypt1 without tag or file_hash", "no way to verify the content");
                summary.failed += 1
            }
        },
        Err(e) => {
            tally.add("mcrypt1 did not decrypt", e);
            summary.failed += 1
        }
    }
}

/// Real name of a `.mcrypt1`, read from its encrypted metadata (base64). On
/// error, returns (category, detail) for diagnostics. The first time, it logs
/// the keys (not the values) of the metadata JSON, to validate the format.
pub(crate) fn mcrypt1_name(key: &BackupKey, meta: &str, logged: &mut bool) -> Result<String, (&'static str, String)> {
    use base64::Engine;
    use base64::engine::general_purpose::STANDARD as B64;

    let raw = B64.decode(meta).map_err(|e| ("metadata is not base64", e.to_string()))?;
    let json = wk_crypt::mcrypt1::decrypt_metadata(key, &raw)
        .map_err(|e| ("metadata did not decrypt", format!("{e} ({} bytes)", raw.len())))?;
    let v: serde_json::Value =
        serde_json::from_slice(&json).map_err(|e| ("decrypted metadata is not JSON", format!("{e} ({} bytes)", json.len())))?;
    if !*logged {
        *logged = true;
        let keys: Vec<&String> = v.as_object().map(|o| o.keys().collect()).unwrap_or_default();
        let name = v.get("name").and_then(|n| n.as_str());
        tracing::info!(keys = ?keys, name_starts_with_media = ?name.map(|n| n.starts_with("Media/")), "metadata of the first .mcrypt1");
    }
    v.get("name")
        .and_then(|n| n.as_str())
        .map(str::to_owned)
        .ok_or(("metadata without the name field", String::from("JSON without name")))
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
        let tmp = dest.with_extension("partial");
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
        .unwrap_or_else(|| format!("media-{}", r.message_id));
    let clean: String = raw
        .chars()
        .map(|c| if c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') { '_' } else { c })
        .collect();
    let clean = clean.trim_start_matches('.').trim();
    if clean.is_empty() { format!("media-{}", r.message_id) } else { clean.chars().take(150).collect() }
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
        assert_eq!(suggested_name(&r(Some("..."), None)), "media-7");
        assert_eq!(suggested_name(&r(None, None)), "media-7");
    }
}
