//! What the user sets in the app (pinned chats), apart from the histories.
//!
//! Its own SQLCipher database (`stores/user-state.db`): re-importing a backup
//! replaces a history, not this. Chats are identified by their WhatsApp
//! identifier (the same one the history keeps), which stays the same across
//! backups of the same account.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags, params};
use wk_import::SourceApp;

use crate::store::open_keyed;
use crate::{Error, Key, fsutil};

const DDL: &str = "
CREATE TABLE IF NOT EXISTS pin (
    app       TEXT NOT NULL,
    jid       TEXT NOT NULL,
    pinned_at INTEGER NOT NULL,
    PRIMARY KEY (app, jid)
) WITHOUT ROWID;
";

/// Path and key of the user state database.
#[derive(Clone)]
pub struct UserState {
    path: PathBuf,
    key: Key,
}

impl std::fmt::Debug for UserState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UserState").field("key", &"<redacted>").finish()
    }
}

impl UserState {
    pub(crate) fn new(stores: &Path, key: Key) -> Self {
        Self { path: stores.join("user-state.db"), key }
    }

    fn open(&self) -> Result<Connection, Error> {
        let created = !self.path.exists();
        let conn = open_keyed(&self.path, &self.key, OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE)?;
        if created {
            fsutil::set_private(&self.path)?;
        }
        conn.execute_batch(DDL)?;
        Ok(conn)
    }

    /// Pinned chats of `app`: identifier → when it was pinned (ms since 1970).
    pub fn pins(&self, app: SourceApp) -> Result<HashMap<String, i64>, Error> {
        if !self.path.exists() {
            return Ok(HashMap::new());
        }
        let conn = self.open()?;
        let mut stmt = conn.prepare("SELECT jid, pinned_at FROM pin WHERE app = ?1")?;
        let map = stmt.query_map([app.as_str()], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>()?;
        Ok(map)
    }

    pub fn set_pinned(&self, app: SourceApp, jid: &str, pinned: bool) -> Result<(), Error> {
        let conn = self.open()?;
        if pinned {
            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0);
            conn.execute(
                "INSERT INTO pin (app, jid, pinned_at) VALUES (?1, ?2, ?3)
                 ON CONFLICT (app, jid) DO UPDATE SET pinned_at = excluded.pinned_at",
                params![app.as_str(), jid, now],
            )?;
        } else {
            conn.execute("DELETE FROM pin WHERE app = ?1 AND jid = ?2", params![app.as_str(), jid])?;
        }
        Ok(())
    }

    /// Forgets everything about `app` (its history was deleted).
    pub fn forget_app(&self, app: SourceApp) -> Result<(), Error> {
        if self.path.exists() {
            self.open()?.execute("DELETE FROM pin WHERE app = ?1", [app.as_str()])?;
        }
        Ok(())
    }
}
