//! One SQLCipher database per source app (`stores/whatsapp.db`, `stores/business.db`).

use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags, OptionalExtension};
use wk_import::{ImportOptions, ImportReport, SourceApp};
use zeroize::Zeroizing;

use crate::{Error, Key, fsutil, random};

/// Summary of an imported database.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoreInfo {
    pub source_app: SourceApp,
    /// Milliseconds since 1970.
    pub imported_at_ms: i64,
    pub source_schema: String,
    pub chats: u64,
    pub messages: u64,
}

/// Path and key of a vault database. Can be cloned to import outside
/// the app state lock. The key is zeroed on drop.
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

    /// Opens the database (read-write). `None` if nothing has been imported yet.
    pub fn open(&self) -> Result<Option<Connection>, Error> {
        if !self.exists() {
            return Ok(None);
        }
        open_keyed(&self.path, &self.key, OpenFlags::SQLITE_OPEN_READ_WRITE).map(Some)
    }

    /// Opens read-only (the viewer). `None` if nothing has been imported yet.
    pub fn open_read_only(&self) -> Result<Option<Connection>, Error> {
        if !self.exists() {
            return Ok(None);
        }
        let conn = open_keyed(&self.path, &self.key, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        // FTS vocabulary (virtual table in `temp`, in memory) for prefix
        // search. Must be created before `query_only`.
        conn.execute_batch(crate::query::VOCAB_DDL)?;
        // 64 MiB cache of decrypted pages (zeroed when freed).
        conn.execute_batch("PRAGMA query_only = ON; PRAGMA cache_size = -65536;")?;
        Ok(Some(conn))
    }

    /// Imports a msgstore into a new database and only then replaces the current one.
    ///
    /// If something fails midway, the previous database stays intact and the
    /// (encrypted) temporary file is deleted.
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
            let report = wk_import::import(src, &mut dst, &ImportOptions { source_app: self.app })?;
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

/// Opens a SQLCipher database with a raw 256-bit key (without SQLCipher's
/// PBKDF2: the key already comes from Argon2id + HKDF).
pub(crate) fn open_keyed(path: &Path, key: &Key, flags: OpenFlags) -> Result<Connection, Error> {
    let conn = Connection::open_with_flags(path, flags | OpenFlags::SQLITE_OPEN_NO_MUTEX)?;

    // `PRAGMA key = "x'<64 hex>'"`. The string is built in a buffer with
    // fixed capacity (it does not reallocate), zeroed on drop.
    let mut sql = Zeroizing::new(String::with_capacity(96));
    sql.push_str("PRAGMA key = \"x'");
    sql.push_str(&hex(key.as_ref()));
    sql.push_str("'\";");
    conn.execute_batch(&sql)?;
    drop(sql);

    // SQLite memory zeroed when freed, and no plaintext temporary files.
    conn.execute_batch("PRAGMA cipher_memory_security = ON; PRAGMA temp_store = MEMORY;")?;

    // A wrong key only shows up on the first page read.
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
