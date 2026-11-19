//! Comandos chamados pelo frontend via `invoke`.
//!
//! Regras:
//! * Senhas e chaves chegam como `String` (não há como evitar a passagem pelo
//!   IPC) e são embrulhadas em `Zeroizing` na entrada.
//! * Nada de chave, senha ou conteúdo de mensagem volta para o webview,
//!   nem em mensagens de erro.
//! * Trabalho pesado (Argon2id, descriptografia, importação) roda em
//!   `spawn_blocking` para não travar a UI.

use std::io::Read;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};
use wa_crypt::{BackupKey, Payload};
use wa_import::SourceApp;
use wa_vault::query::{self, Anchor, Around, Chat, Cursor, Direction, Message, SearchHit};
use wa_vault::{KdfParams, Vault};
use zeroize::Zeroizing;

use crate::error::CmdError;
use crate::state::{AUTOLOCK_AFTER, AppState};

const EVENT_IMPORT_PROGRESS: &str = "import-progress";

pub(crate) async fn blocking<T, F>(f: F) -> Result<T, CmdError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, CmdError> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|_| CmdError::new("internal", "a operação foi interrompida"))?
}

// ---------------------------------------------------------------- cofre

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultStatus {
    /// `none` (não criado), `locked` ou `unlocked`.
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
    blocking(move || {
        let state = app.state::<AppState>();
        let vault = Vault::create(&state.vault_dir, &password, KdfParams::DEFAULT)?;
        *state.vault() = Some(vault);
        state.touch();
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn vault_unlock(app: AppHandle, password: String) -> Result<(), CmdError> {
    let password = Zeroizing::new(password);
    blocking(move || {
        let state = app.state::<AppState>();
        let vault = Vault::unlock(&state.vault_dir, &password)?;
        *state.vault() = Some(vault);
        state.touch();
        Ok(())
    })
    .await
}

#[tauri::command]
pub fn vault_lock(app: AppHandle) {
    crate::state::lock_all(&app);
}

/// A UI chama isto (com throttle) a cada interação do usuário.
#[tauri::command]
pub fn vault_touch(state: State<'_, AppState>) {
    state.touch();
}

#[tauri::command]
pub async fn vault_change_password(app: AppHandle, current: String, next: String) -> Result<(), CmdError> {
    let current = Zeroizing::new(current);
    let next = Zeroizing::new(next);
    blocking(move || {
        let state = app.state::<AppState>();
        let mut guard = state.vault();
        let vault = guard.as_mut().ok_or_else(CmdError::locked)?;
        vault.change_password(&current, &next, KdfParams::DEFAULT)?;
        state.touch();
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
    blocking(move || {
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

// ---------------------------------------------------------------- importação

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupInfo {
    version: &'static str,
    size_bytes: u64,
}

/// Identifica o formato (crypt14/crypt15) lendo só o header. Não precisa da
/// chave e não descriptografa nada.
#[tauri::command]
pub fn inspect_backup(path: String) -> Result<BackupInfo, CmdError> {
    let file = std::fs::File::open(&path)?;
    let size_bytes = file.metadata()?.len();
    let mut head = Vec::with_capacity(4096);
    file.take(4096).read_to_end(&mut head)?;
    let prefix = wa_crypt::parse_prefix(&head)?;
    Ok(BackupInfo {
        version: match prefix.version {
            wa_crypt::CryptVersion::Crypt14 => "crypt14",
            wa_crypt::CryptVersion::Crypt15 => "crypt15",
        },
        size_bytes,
    })
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum KeyInput {
    /// Os 64 dígitos digitados pelo usuário.
    Hex { value: String },
    /// Caminho do `key` (crypt14) ou do `encrypted_backup.key` (crypt15).
    File { path: String },
}

impl KeyInput {
    pub(crate) fn load(self) -> Result<BackupKey, CmdError> {
        Ok(match self {
            KeyInput::Hex { value } => BackupKey::from_hex(&Zeroizing::new(value))?,
            KeyInput::File { path } => {
                let meta = std::fs::metadata(&path)?;
                // Arquivos de chave têm dezenas de bytes. Recusa arquivos
                // grandes antes de ler (provavelmente o arquivo errado).
                if meta.len() > 4096 {
                    return Err(wa_crypt::Error::UnknownKeyFile.into());
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
    /// `[tipo, quantidade]` de tipos de mensagem que o importer não conhece.
    unknown_types: Vec<(i64, u64)>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Progress {
    phase: &'static str,
}

/// Lê o backup, descriptografa em memória, importa para o cofre e descarta o
/// texto puro. O msgstore descriptografado nunca é gravado em disco.
#[tauri::command]
pub async fn import_backup(
    app: AppHandle,
    backup_path: String,
    key: KeyInput,
    source_app: String,
) -> Result<ImportSummary, CmdError> {
    blocking(move || {
        let state = app.state::<AppState>();
        let _busy = state.busy();
        let source_app = self::source_app(&source_app)?;
        progress(&app, "reading");
        let key = key.load()?;
        let file = std::fs::read(&backup_path)?;
        import_encrypted(&app, &state, file, key, source_app)
    })
    .await
}

pub(crate) fn progress(app: &AppHandle, phase: &'static str) {
    let _ = app.emit(EVENT_IMPORT_PROGRESS, Progress { phase });
}

/// Descriptografa um backup (já em memória) e importa para o cofre. O texto
/// puro nunca vai para o disco: fica em buffers zerados e vai direto para o
/// banco SQLCipher.
pub(crate) fn import_encrypted(
    app: &AppHandle,
    state: &AppState,
    file: Vec<u8>,
    key: BackupKey,
    source_app: SourceApp,
) -> Result<ImportSummary, CmdError> {
    let store = state.vault().as_ref().ok_or_else(CmdError::locked)?.store(source_app)?;

    progress(app, "decrypting");
    let decrypted = wa_crypt::decrypt(&file, &key)?;
    drop((file, key));
    if decrypted.payload != Payload::Sqlite {
        return Err(wa_import::Error::NotMsgstore.into());
    }

    progress(app, "importing");
    // `open_msgstore` copia os bytes para a memória do SQLite (zerada ao
    // liberar). A cópia do wa-crypt é zerada logo em seguida.
    let src = wa_import::open_msgstore(&decrypted.data)?;
    drop(decrypted);
    // A conexão do visualizador aponta para o arquivo antigo (e no Windows
    // impediria a troca do arquivo).
    state.forget_store(source_app);
    let report = store.import(&src);
    state.forget_store(source_app);
    let report = report?;
    drop(src);

    progress(app, "done");
    Ok(ImportSummary {
        source_schema: report.source_schema.map(|s| match s {
            wa_import::SourceSchema::Modern => "modern",
            wa_import::SourceSchema::Legacy => "legacy",
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

// ---------------------------------------------------------------- visualizador

pub(crate) fn source_app(s: &str) -> Result<SourceApp, CmdError> {
    SourceApp::parse(s).ok_or_else(|| CmdError::new("invalid_argument", "app de origem desconhecido"))
}

#[tauri::command]
pub async fn list_chats(app: AppHandle, store: String) -> Result<Vec<Chat>, CmdError> {
    blocking(move || {
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
    blocking(move || {
        let store = source_app(&store)?;
        app.state::<AppState>()
            .with_store(store, |c| Ok(query::messages(c, chat_id, direction, cursor, limit)?))
    })
    .await
}

#[tauri::command]
pub async fn get_messages_around(
    app: AppHandle,
    store: String,
    chat_id: i64,
    anchor: Anchor,
    limit: u32,
) -> Result<Around, CmdError> {
    blocking(move || {
        let store = source_app(&store)?;
        app.state::<AppState>()
            .with_store(store, |c| Ok(query::messages_around(c, chat_id, anchor, limit)?))
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
    blocking(move || {
        let store = source_app(&store)?;
        app.state::<AppState>()
            .with_store(store, |c| Ok(query::search(c, &query, chat_id, before, limit)?))
    })
    .await
}
