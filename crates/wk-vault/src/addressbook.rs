//! The address book: names for phone numbers, from a Google Contacts export.
//!
//! Kept in its own SQLCipher database (`stores/address-book.db`), apart from
//! the histories: re-importing a backup replaces a history, and the names are
//! applied to it again. Only normalized numbers and display names are kept
//! (never the imported file).

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{OpenFlags, OptionalExtension, params};
use wk_import::contacts::variants;

use crate::store::{hex, open_keyed, remove_with_journal};
use crate::{Error, Key, StoreHandle, fsutil, random};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AddressBookInfo {
    /// People with at least one usable number.
    pub people: u64,
    pub numbers: u64,
    /// Milliseconds since 1970.
    pub imported_at_ms: i64,
}

/// Path and key of the address book.
#[derive(Clone)]
pub struct AddressBook {
    path: PathBuf,
    key: Key,
}

impl std::fmt::Debug for AddressBook {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AddressBook").field("key", &"<redacted>").finish()
    }
}

const DDL: &str = "
CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL) WITHOUT ROWID;
CREATE TABLE number (phone TEXT PRIMARY KEY, name TEXT NOT NULL) WITHOUT ROWID;
";

impl AddressBook {
    pub(crate) fn new(stores: &Path, key: Key) -> Self {
        Self { path: stores.join("address-book.db"), key }
    }

    pub fn exists(&self) -> bool {
        self.path.is_file()
    }

    /// Replaces the address book with `numbers` (normalized phone → name) for
    /// `people` people. Atomic: a new encrypted file is renamed over the old one.
    pub fn replace(&self, numbers: &HashMap<String, String>, people: u64) -> Result<AddressBookInfo, Error> {
        let suffix: [u8; 6] = random()?;
        let tmp = self.path.with_extension(format!("db.tmp-{}", hex(&suffix).as_str()));
        let imported_at_ms =
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0);
        let result = (|| {
            let mut conn = open_keyed(&tmp, &self.key, OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE)?;
            fsutil::set_private(&tmp)?;
            conn.execute_batch(DDL)?;
            let tx = conn.transaction()?;
            {
                let mut insert = tx.prepare("INSERT INTO number (phone, name) VALUES (?1, ?2)")?;
                for (phone, name) in numbers {
                    insert.execute(params![phone, name])?;
                }
                let mut meta = tx.prepare("INSERT INTO meta (key, value) VALUES (?1, ?2)")?;
                meta.execute(params!["people", people.to_string()])?;
                meta.execute(params!["imported_at", imported_at_ms.to_string()])?;
            }
            tx.commit()?;
            conn.close().map_err(|(_, e)| e)?;
            fs::File::open(&tmp)?.sync_all()?;
            Ok(())
        })();
        match result {
            Ok(()) => {
                fs::rename(&tmp, &self.path)?;
                Ok(AddressBookInfo { people, numbers: numbers.len() as u64, imported_at_ms })
            }
            Err(e) => {
                remove_with_journal(&tmp);
                Err(e)
            }
        }
    }

    /// Normalized phone → name. `None` if there is no address book.
    pub fn load(&self) -> Result<Option<HashMap<String, String>>, Error> {
        if !self.exists() {
            return Ok(None);
        }
        let conn = open_keyed(&self.path, &self.key, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let mut stmt = conn.prepare("SELECT phone, name FROM number")?;
        let map = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>()?;
        Ok(Some(map))
    }

    pub fn info(&self) -> Result<Option<AddressBookInfo>, Error> {
        if !self.exists() {
            return Ok(None);
        }
        let conn = open_keyed(&self.path, &self.key, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let meta = |key: &str| -> Result<i64, Error> {
            Ok(conn
                .query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| r.get::<_, String>(0))
                .optional()?
                .and_then(|v| v.parse().ok())
                .unwrap_or(0))
        };
        let numbers: i64 = conn.query_row("SELECT count(*) FROM number", [], |r| r.get(0))?;
        Ok(Some(AddressBookInfo { people: meta("people")? as u64, numbers: numbers as u64, imported_at_ms: meta("imported_at")? }))
    }

    pub fn delete(&self) -> Result<(), Error> {
        fsutil::remove_database(&self.path)?;
        Ok(())
    }
}

/// What applying the address book to a history did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NamesApplied {
    /// People (contacts with a number) that got a name.
    pub contacts: u64,
    /// One-to-one chats (that are listed) with a name.
    pub chats: u64,
}

impl StoreHandle {
    /// Writes the address book's names into this history (`None` clears them).
    /// Each number is looked up in its WhatsApp forms (see
    /// [`wk_import::contacts::variants`]).
    pub fn apply_names(&self, book: Option<&HashMap<String, String>>) -> Result<NamesApplied, Error> {
        let Some(mut conn) = self.open()? else { return Ok(NamesApplied::default()) };
        let tx = conn.transaction()?;
        tx.execute("UPDATE contact SET name = NULL WHERE name IS NOT NULL", [])?;
        let mut applied = NamesApplied::default();
        if let Some(book) = book {
            let people: Vec<(i64, String)> = tx
                .prepare("SELECT id, phone FROM contact WHERE phone IS NOT NULL")?
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<Result<_, _>>()?;
            let mut update = tx.prepare("UPDATE contact SET name = ?1 WHERE id = ?2")?;
            for (id, phone) in people {
                if let Some(name) = variants(&phone).iter().find_map(|v| book.get(v)) {
                    update.execute(params![name, id])?;
                    applied.contacts += 1;
                }
            }
            drop(update);
            applied.chats = tx.query_row(
                "SELECT count(*) FROM chat c JOIN contact ct ON ct.id = c.contact_id
                 WHERE c.kind = 'direct' AND ct.name IS NOT NULL
                   AND EXISTS (SELECT 1 FROM message m WHERE m.chat_id = c.id AND m.kind <> 'system')",
                [],
                |r| r.get::<_, i64>(0),
            )? as u64;
        }
        tx.commit()?;
        Ok(applied)
    }
}
