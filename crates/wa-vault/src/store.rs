//! Um banco SQLCipher por app de origem (`stores/whatsapp.db`, `stores/business.db`).

use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags, OptionalExtension};
use wa_import::{ImportOptions, ImportReport, SourceApp};
use zeroize::Zeroizing;

use crate::{Error, Key, fsutil, random};

/// Resumo de um banco importado.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoreInfo {
    pub source_app: SourceApp,
    /// Milissegundos desde 1970.
    pub imported_at_ms: i64,
    pub source_schema: String,
    pub chats: u64,
    pub messages: u64,
}

/// Caminho e chave de um banco do cofre. Pode ser clonado para importar fora
/// do lock do estado do app. A chave é zerada no drop.
#[derive(Clone)]
pub struct StoreHandle {
    app: SourceApp,
    path: PathBuf,
    key: Key,
}

impl std::fmt::Debug for StoreHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StoreHandle")
            .field("app", &self.app)
            .field("key", &"<redacted>")
            .finish()
    }
}

impl StoreHandle {
    pub(crate) fn new(app: SourceApp, dir: &Path, key: Key) -> Self {
        Self { app, path: dir.join(format!("{}.db", app.as_str())), key }
    }

    pub fn app(&self) -> SourceApp {
        self.app
    }

    pub fn exists(&self) -> bool {
        self.path.is_file()
    }

    /// Abre o banco (leitura e escrita). `None` se ainda não houve importação.
    pub fn open(&self) -> Result<Option<Connection>, Error> {
        if !self.exists() {
            return Ok(None);
        }
        open_keyed(&self.path, &self.key, OpenFlags::SQLITE_OPEN_READ_WRITE).map(Some)
    }

    /// Importa um msgstore para um banco novo e só então substitui o atual.
    ///
    /// Se algo falhar no meio, o banco anterior continua intacto e o arquivo
    /// temporário (cifrado) é apagado.
    pub fn import(&self, src: &Connection) -> Result<ImportReport, Error> {
        let suffix: [u8; 6] = random()?;
        let tmp = self.path.with_extension(format!("db.tmp-{}", hex(&suffix).as_str()));

        let result = (|| {
            let mut dst = open_keyed(
                &tmp,
                &self.key,
                OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
            )?;
            fsutil::set_private(&tmp)?;
            let report = wa_import::import(src, &mut dst, &ImportOptions { source_app: self.app })?;
            dst.close().map_err(|(_, e)| e)?;
            fs::File::open(&tmp)?.sync_all()?;
            Ok(report)
        })();

        match result {
            Ok(report) => {
                fs::rename(&tmp, &self.path)?;
                Ok(report)
            }
            Err(e) => {
                remove_with_journal(&tmp);
                Err(e)
            }
        }
    }

    pub fn info(&self) -> Result<Option<StoreInfo>, Error> {
        let Some(conn) = self.open()? else { return Ok(None) };
        let meta = |key: &str| -> Result<Option<String>, Error> {
            Ok(conn
                .query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| r.get(0))
                .optional()?)
        };
        let count = |table: &str| -> Result<u64, Error> {
            Ok(conn.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get::<_, i64>(0))? as u64)
        };
        Ok(Some(StoreInfo {
            source_app: self.app,
            imported_at_ms: meta("imported_at")?.and_then(|v| v.parse().ok()).unwrap_or(0),
            source_schema: meta("source_schema")?.unwrap_or_default(),
            chats: count("chat")?,
            messages: count("message")?,
        }))
    }
}

/// Abre um banco SQLCipher com chave bruta de 256 bits (sem o PBKDF2 do
/// SQLCipher: a chave já vem de Argon2id + HKDF).
pub(crate) fn open_keyed(path: &Path, key: &Key, flags: OpenFlags) -> Result<Connection, Error> {
    let conn = Connection::open_with_flags(path, flags | OpenFlags::SQLITE_OPEN_NO_MUTEX)?;

    // `PRAGMA key = "x'<64 hex>'"`. A string é montada num buffer com
    // capacidade fixa (não realoca) e zerado no drop.
    let mut sql = Zeroizing::new(String::with_capacity(96));
    sql.push_str("PRAGMA key = \"x'");
    sql.push_str(&hex(key.as_ref()));
    sql.push_str("'\";");
    conn.execute_batch(&sql)?;
    drop(sql);

    // Memória do SQLite zerada ao liberar, e nada de arquivos temporários em claro.
    conn.execute_batch("PRAGMA cipher_memory_security = ON; PRAGMA temp_store = MEMORY;")?;

    // Chave errada só aparece na primeira leitura de página.
    conn.query_row("SELECT count(*) FROM sqlite_master", [], |_| Ok(()))
        .map_err(|e| match e.sqlite_error_code() {
            Some(rusqlite::ErrorCode::NotADatabase) => Error::StoreUnreadable,
            _ => Error::Sqlite(e),
        })?;
    Ok(conn)
}

fn hex(bytes: &[u8]) -> Zeroizing<String> {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut s = Zeroizing::new(String::with_capacity(bytes.len() * 2));
    for b in bytes {
        s.push(DIGITS[(b >> 4) as usize] as char);
        s.push(DIGITS[(b & 0xf) as usize] as char);
    }
    s
}

pub(crate) fn remove_with_journal(path: &Path) {
    let _ = fs::remove_file(path);
    let mut journal = path.as_os_str().to_owned();
    journal.push("-journal");
    let _ = fs::remove_file(PathBuf::from(journal));
}
