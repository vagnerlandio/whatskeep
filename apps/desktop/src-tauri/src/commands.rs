//! Commands called by the frontend via `invoke`.
//!
//! Rules:
//! * Passwords and keys arrive as `String` (passing through the IPC cannot be
//!   avoided) and are wrapped in `Zeroizing` on entry.
//! * No key, password or message content goes back to the webview, not even
//!   in error messages.
//! * Heavy work (Argon2id, decryption, import) runs in `spawn_blocking` so it
//!   does not freeze the UI.

use std::io::Read;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};
use wk_crypt::{BackupKey, Payload};
use wk_import::SourceApp;
use wk_vault::query::{self, Anchor, Around, Chat, Cursor, Direction, Message, SearchHit};
use wk_vault::{KdfParams, Vault};
use zeroize::Zeroizing;

use crate::error::CmdError;
use crate::state::{AUTOLOCK_AFTER, AppState};

const EVENT_IMPORT_PROGRESS: &str = "import-progress";

/// Runs a command off the UI thread and logs its name, duration and result.
/// Frequent viewer commands (scrolling) are logged at `trace` when they succeed.
pub(crate) async fn blocking<T, F>(name: &'static str, f: F) -> Result<T, CmdError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, CmdError> + Send + 'static,
{
    let started = std::time::Instant::now();
    tracing::trace!(command = name, "start");
    let result = tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|_| CmdError::new("internal", "a operação foi interrompida"))
        .and_then(|r| r);
    let ms = started.elapsed().as_millis() as u64;
    match &result {
        Ok(_) if matches!(name, "get_messages" | "get_messages_around" | "list_chats" | "list_stores") => {
            tracing::trace!(command = name, ms, "ok")
        }
        Ok(_) => tracing::info!(command = name, ms, "ok"),
        Err(e) if matches!(e.code, "cancelled" | "locked") => {
            tracing::info!(command = name, ms, code = e.code, "{}", e.message)
        }
        Err(e) => tracing::warn!(command = name, ms, code = e.code, "error: {}", e.message),
    }
    result
}

// ---------------------------------------------------------------- vault

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultStatus {
    /// `none` (not created), `locked` or `unlocked`.
    state: &'static str,
    autolock_secs: u64,
}

#[tauri::command]
pub fn vault_status(state: State<'_, AppState>) -> VaultStatus {
    let s = if state.vault().is_some() {
        "unlocked"
    } else if Vault::exists(&state.vault_dir) {
        "locked"
    } else {
        "none"
    };
    VaultStatus { state: s, autolock_secs: AUTOLOCK_AFTER.as_secs() }
}

#[tauri::command]
pub async fn vault_create(app: AppHandle, password: String) -> Result<(), CmdError> {
    let password = Zeroizing::new(password);
    blocking("vault_create", move || {
        let state = app.state::<AppState>();
        let t = crate::diag::timer("create vault (Argon2id)");
        let vault = Vault::create(&state.vault_dir, &password, KdfParams::DEFAULT)?;
        tracing::info!(ms = t.ms() as u64, "vault created");
        *state.vault() = Some(vault);
        state.touch();
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn vault_unlock(app: AppHandle, password: String) -> Result<(), CmdError> {
    let password = Zeroizing::new(password);
    blocking("vault_unlock", move || {
        let state = app.state::<AppState>();
        let delay = state.unlock_delay();
        if !delay.is_zero() {
            tracing::info!(wait_s = delay.as_secs(), "delay due to consecutive wrong passwords");
        }
        std::thread::sleep(delay);
        let t = crate::diag::timer("unlock (Argon2id)");
        let result = Vault::unlock(&state.vault_dir, &password);
        tracing::info!(ms = t.ms() as u64, ok = result.is_ok(), "unlock attempt");
        state.unlock_result(!matches!(result, Err(wk_vault::Error::WrongPassword)));
        *state.vault() = Some(result?);
        state.touch();
        Ok(())
    })
    .await
}

#[tauri::command]
pub fn vault_lock(app: AppHandle) {
    tracing::info!("vault locked by the user");
    crate::state::lock_all(&app);
}

/// The UI calls this (throttled) on every user interaction.
#[tauri::command]
pub fn vault_touch(state: State<'_, AppState>) {
    state.touch();
}

#[tauri::command]
pub async fn vault_change_password(app: AppHandle, current: String, next: String) -> Result<(), CmdError> {
    let current = Zeroizing::new(current);
    let next = Zeroizing::new(next);
    blocking("vault_change_password", move || {
        let state = app.state::<AppState>();
        let mut guard = state.vault();
        let vault = guard.as_mut().ok_or_else(CmdError::locked)?;
        vault.change_password(&current, &next, KdfParams::DEFAULT)?;
        state.touch();
        Ok(())
    })
    .await
}

/// Bytes the vault takes on disk (shown before deleting it).
#[tauri::command]
pub async fn vault_usage(app: AppHandle) -> Result<u64, CmdError> {
    blocking("vault_usage", move || Ok(Vault::disk_usage(&app.state::<AppState>().vault_dir))).await
}

/// Deletes the whole vault, open or locked: it is also the way out for a
/// forgotten password. Backup copies saved to folders are not touched. Not
/// while an import or download runs.
#[tauri::command]
pub async fn vault_delete(app: AppHandle) -> Result<(), CmdError> {
    blocking("vault_delete", move || {
        let state = app.state::<AppState>();
        let _job = state.start_job()?;
        crate::state::lock_all(&app);
        let bytes = Vault::disk_usage(&state.vault_dir);
        Vault::destroy(&state.vault_dir)?;
        state.unlock_result(true); // a new vault starts without the wrong-password delay
        tracing::info!(size = %crate::diag::mb(bytes), "vault deleted by the user");
        Ok(())
    })
    .await
}

/// Deletes one app's history (messages and media) and keeps the rest.
#[tauri::command]
pub async fn store_delete(app: AppHandle, store: String) -> Result<(), CmdError> {
    blocking("store_delete", move || {
        let target = source_app(&store)?;
        let state = app.state::<AppState>();
        let _job = state.start_job()?;
        state.forget_store(target);
        state.forget_media(target);
        state.vault().as_ref().ok_or_else(CmdError::locked)?.delete_store(target)?;
        tracing::info!(app = target.as_str(), "history deleted by the user");
        Ok(())
    })
    .await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoreDto {
    source_app: &'static str,
    imported_at_ms: i64,
    source_schema: String,
    chats: u64,
    messages: u64,
}

#[tauri::command]
pub async fn list_stores(app: AppHandle) -> Result<Vec<StoreDto>, CmdError> {
    blocking("list_stores", move || {
        let state = app.state::<AppState>();
        let guard = state.vault();
        let vault = guard.as_ref().ok_or_else(CmdError::locked)?;
        Ok(vault
            .stores()?
            .into_iter()
            .map(|s| StoreDto {
                source_app: s.source_app.as_str(),
                imported_at_ms: s.imported_at_ms,
                source_schema: s.source_schema,
                chats: s.chats,
                messages: s.messages,
            })
            .collect())
    })
    .await
}

// ---------------------------------------------------------------- import

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupInfo {
    version: &'static str,
    size_bytes: u64,
    /// The file is in a backup folder (`.../Databases/msgstore.db.crypt15`)
    /// with a `Media/` folder next to it: its media can come in too.
    media_folder: bool,
}

/// The backup folder of a database file: `<root>/Databases/<file>` with
/// `<root>/Media/` present (a folder copied from the phone, or a Drive backup
/// the app saved).
pub(crate) fn backup_media_root(db_path: &std::path::Path) -> Option<std::path::PathBuf> {
    let databases = db_path.parent()?;
    if databases.file_name()? != "Databases" {
        return None;
    }
    let root = databases.parent()?;
    root.join("Media").is_dir().then(|| root.to_path_buf())
}

/// Identifies the format (crypt14/crypt15) by reading only the header. Does
/// not need the key and decrypts nothing.
#[tauri::command]
pub fn inspect_backup(path: String) -> Result<BackupInfo, CmdError> {
    let file = std::fs::File::open(&path)?;
    let size_bytes = file.metadata()?.len();
    let mut head = Vec::with_capacity(4096);
    file.take(4096).read_to_end(&mut head)?;
    let prefix = wk_crypt::parse_prefix(&head).inspect_err(|e| tracing::warn!("inspect_backup: header not recognized: {e}"))?;
    tracing::info!(version = ?prefix.version, header_bytes = prefix.header_len, size = %crate::diag::mb(size_bytes), "inspect_backup");
    Ok(BackupInfo {
        version: match prefix.version {
            wk_crypt::CryptVersion::Crypt14 => "crypt14",
            wk_crypt::CryptVersion::Crypt15 => "crypt15",
        },
        size_bytes,
        media_folder: backup_media_root(std::path::Path::new(&path)).is_some(),
    })
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum KeyInput {
    /// The 64 digits typed by the user.
    Hex { value: String },
    /// Path of the `key` (crypt14) or `encrypted_backup.key` (crypt15) file.
    File { path: String },
}

impl KeyInput {
    pub(crate) fn load(self) -> Result<BackupKey, CmdError> {
        Ok(match self {
            KeyInput::Hex { value } => BackupKey::from_hex(&Zeroizing::new(value))?,
            KeyInput::File { path } => {
                let meta = std::fs::metadata(&path)?;
                // Key files are tens of bytes. Refuses large files before
                // reading them (probably the wrong file).
                if meta.len() > 4096 {
                    return Err(wk_crypt::Error::UnknownKeyFile.into());
                }
                BackupKey::from_key_file(&Zeroizing::new(std::fs::read(&path)?))?
            }
        })
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSummary {
    source_schema: Option<&'static str>,
    chats: u64,
    messages: u64,
    media: u64,
    reactions: u64,
    vcards: u64,
    skipped_status: u64,
    skipped_orphans: u64,
    /// `[type, count]` of message types the importer does not know.
    unknown_types: Vec<(i64, u64)>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Progress {
    phase: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    summary: ImportSummary,
    media: Option<crate::media_import::MediaSummary>,
}

/// Reads the backup, decrypts it in memory, imports it into the vault and
/// discards the plaintext. The decrypted msgstore is never written to disk.
/// With `with_media`, then imports the media of the backup folder the file is
/// in (see [`backup_media_root`]), `.mcrypt1` included.
#[tauri::command]
pub async fn import_backup(
    app: AppHandle,
    backup_path: String,
    key: KeyInput,
    source_app: String,
    with_media: bool,
) -> Result<ImportResult, CmdError> {
    blocking("import_backup", move || {
        let state = app.state::<AppState>();
        let _job = state.start_job()?;
        let source_app = self::source_app(&source_app)?;
        progress(&app, "reading");
        let key = key.load()?;
        let media_root = if with_media { backup_media_root(std::path::Path::new(&backup_path)) } else { None };
        tracing::info!(key = ?key.kind(), app = source_app.as_str(), with_media = media_root.is_some(), "import local backup");
        let file = std::fs::read(&backup_path)?;
        let summary = import_encrypted(&app, &state, file, &key, source_app)?;
        let media = match media_root {
            Some(root) => Some(crate::media_import::import_folder(&app, source_app, &root, Some(&key))?),
            None => None,
        };
        Ok(ImportResult { summary, media })
    })
    .await
}

pub(crate) fn progress(app: &AppHandle, phase: &'static str) {
    let _ = app.emit(EVENT_IMPORT_PROGRESS, Progress { phase });
}

/// Decrypts a backup (already in memory) and imports it into the vault. The
/// plaintext never goes to disk: it stays in zeroed buffers and goes straight
/// into the SQLCipher database.
pub(crate) fn import_encrypted(
    app: &AppHandle,
    state: &AppState,
    file: Vec<u8>,
    key: &BackupKey,
    source_app: SourceApp,
) -> Result<ImportSummary, CmdError> {
    let store = state.vault().as_ref().ok_or_else(CmdError::locked)?.store(source_app)?;

    progress(app, "decrypting");
    let size = file.len() as u64;
    let t = crate::diag::timer("decrypt");
    let decrypted = wk_crypt::decrypt(&file, key).inspect_err(|e| tracing::warn!(size = %crate::diag::mb(size), "decryption failed: {e}"))?;
    drop(file);
    tracing::info!(
        version = ?decrypted.version,
        multifile = decrypted.multifile,
        content = ?decrypted.payload,
        encrypted = %crate::diag::mb(size),
        sqlite = %crate::diag::mb(decrypted.data.len() as u64),
        ms = t.ms() as u64,
        "backup decrypted"
    );
    drop(t);
    if decrypted.payload != Payload::Sqlite {
        return Err(wk_import::Error::NotMsgstore.into());
    }

    progress(app, "importing");
    // `open_msgstore` copies the bytes into SQLite's memory (zeroed when
    // freed). The wk-crypt copy is zeroed right after.
    let src = wk_import::open_msgstore(&decrypted.data)?;
    drop(decrypted);
    // The viewer's connection points to the old file (and on Windows it would
    // prevent the file from being replaced).
    state.forget_store(source_app);
    let t = crate::diag::timer("import into the vault");
    let report = store.import(&src);
    state.forget_store(source_app);
    let report = report.inspect_err(|e| tracing::warn!("import failed: {e}"))?;
    drop(src);
    tracing::info!(
        app = source_app.as_str(),
        schema = ?report.source_schema,
        chats = report.chats,
        messages = report.messages,
        media = report.media,
        reactions = report.reactions,
        vcards = report.vcards,
        skipped_status = report.skipped_status,
        orphans = report.skipped_orphans,
        ms = t.ms() as u64,
        "import done"
    );
    if !report.unknown_types.is_empty() {
        tracing::warn!(types = ?report.unknown_types, "unknown message types (message_type → count)");
    }
    drop(t);

    // The address book's names, into the new history.
    crate::contacts::apply_after_import(state, source_app);

    progress(app, "done");
    Ok(ImportSummary {
        source_schema: report.source_schema.map(|s| match s {
            wk_import::SourceSchema::Modern => "modern",
            wk_import::SourceSchema::Legacy => "legacy",
        }),
        chats: report.chats,
        messages: report.messages,
        media: report.media,
        reactions: report.reactions,
        vcards: report.vcards,
        skipped_status: report.skipped_status,
        skipped_orphans: report.skipped_orphans,
        unknown_types: report.unknown_types.into_iter().collect(),
    })
}

// ---------------------------------------------------------------- viewer

pub(crate) fn source_app(s: &str) -> Result<SourceApp, CmdError> {
    SourceApp::parse(s).ok_or_else(|| CmdError::new("invalid_argument", "app de origem desconhecido"))
}

#[tauri::command]
pub async fn list_chats(app: AppHandle, store: String) -> Result<Vec<Chat>, CmdError> {
    blocking("list_chats", move || {
        let store = source_app(&store)?;
        app.state::<AppState>().with_store(store, |c| Ok(query::chats(c)?))
    })
    .await
}

#[tauri::command]
pub async fn get_messages(
    app: AppHandle,
    store: String,
    chat_id: i64,
    direction: Direction,
    cursor: Option<Cursor>,
    limit: u32,
) -> Result<Vec<Message>, CmdError> {
    blocking("get_messages", move || {
        let store = source_app(&store)?;
        let state = app.state::<AppState>();
        let mut page = state.with_store(store, |c| Ok(query::messages(c, chat_id, direction, cursor, limit)?))?;
        mark_available(&state, store, &mut page)?;
        Ok(page)
    })
    .await
}

/// Marks which media in the page have their file in the vault.
fn mark_available(state: &AppState, store: SourceApp, page: &mut [Message]) -> Result<(), CmdError> {
    state.with_media(store, |reader| {
        let Some(reader) = reader else { return Ok(()) };
        for m in page.iter_mut() {
            if let Some(media) = m.media.as_mut() {
                media.available = m.kind != "deleted"
                    && reader.find(media.file_hash.as_deref(), media.file_path.as_deref())?.is_some();
            }
        }
        Ok(())
    })
}

#[tauri::command]
pub async fn get_messages_around(
    app: AppHandle,
    store: String,
    chat_id: i64,
    anchor: Anchor,
    limit: u32,
) -> Result<Around, CmdError> {
    blocking("get_messages_around", move || {
        let store = source_app(&store)?;
        let state = app.state::<AppState>();
        let mut around = state.with_store(store, |c| Ok(query::messages_around(c, chat_id, anchor, limit)?))?;
        mark_available(&state, store, &mut around.messages)?;
        Ok(around)
    })
    .await
}

#[tauri::command]
pub async fn search_messages(
    app: AppHandle,
    store: String,
    query: String,
    chat_id: Option<i64>,
    before: Option<Cursor>,
    limit: u32,
) -> Result<Vec<SearchHit>, CmdError> {
    blocking("search_messages", move || {
        let store = source_app(&store)?;
        app.state::<AppState>()
            .with_store(store, |c| Ok(query::search(c, &query, chat_id, before, limit)?))
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_root_of_a_backup_folder() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("whatsapp-backup-2026-11-19");
        std::fs::create_dir_all(root.join("Databases")).unwrap();
        let db = root.join("Databases/msgstore.db.crypt15");
        assert_eq!(backup_media_root(&db), None, "no Media/ folder yet");
        std::fs::create_dir_all(root.join("Media")).unwrap();
        assert_eq!(backup_media_root(&db), Some(root.clone()));
        assert_eq!(backup_media_root(&dir.path().join("msgstore.db.crypt15")), None);
        assert_eq!(backup_media_root(&root.join("Other/msgstore.db.crypt15")), None);
    }
}
