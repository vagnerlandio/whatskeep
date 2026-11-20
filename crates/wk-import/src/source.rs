use std::collections::{HashMap, HashSet};

use rusqlite::{Connection, MAIN_DB};

use crate::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceSchema {
    /// `message`, `chat`, `jid`, `message_media`... (2021+)
    Modern,
    /// `messages`, `chat_list`...
    Legacy,
}

impl SourceSchema {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            SourceSchema::Modern => "modern",
            SourceSchema::Legacy => "legacy",
        }
    }
}

/// Opens a decrypted msgstore straight from memory, read-only,
/// without touching the disk.
///
/// Turns on SQLCipher's `cipher_memory_security`: all memory allocated by
/// SQLite (the database copy and the page cache) is zeroed before being
/// freed. The setting applies to the whole process.
pub fn open_msgstore(bytes: &[u8]) -> Result<Connection, Error> {
    let mut conn = Connection::open_in_memory()?;
    // temp_store = MEMORY: large sorts (the import sorts by date) must not
    // turn into temporary files with plaintext in TMPDIR.
    conn.execute_batch("PRAGMA cipher_memory_security = ON; PRAGMA temp_store = MEMORY;")?;
    conn.deserialize_read_exact(MAIN_DB, bytes, bytes.len(), true)?;
    // Fail early if the bytes are not a valid SQLite database.
    conn.query_row("SELECT count(*) FROM sqlite_master", [], |_| Ok(()))
        .map_err(|_| Error::NotMsgstore)?;
    Ok(conn)
}

/// Tables and columns present in the msgstore, to build version-tolerant queries.
pub(crate) struct SourceInfo {
    pub kind: SourceSchema,
    tables: HashMap<String, HashSet<String>>,
}

impl SourceInfo {
    pub fn load(conn: &Connection) -> Result<Self, Error> {
        let mut tables = HashMap::new();
        let names: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table'")?
            .query_map([], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        for name in names {
            let cols: HashSet<String> = conn
                .prepare(&format!("PRAGMA table_info(\"{}\")", name.replace('"', "\"\"")))?
                .query_map([], |r| r.get::<_, String>(1))?
                .collect::<Result<_, _>>()?;
            tables.insert(name, cols);
        }

        let has = |t: &str, c: &str| tables.get(t).is_some_and(|cols| cols.contains(c));
        let count = |t: &str| -> i64 {
            conn.query_row(&format!("SELECT count(*) FROM {t}"), [], |r| r.get(0))
                .unwrap_or(0)
        };
        // Transitional databases have both tables. The one with messages wins,
        // and the modern schema on a tie.
        let modern = has("message", "chat_row_id") && has("chat", "jid_row_id") && tables.contains_key("jid");
        let legacy = has("messages", "key_remote_jid");
        let kind = match (modern, legacy) {
            (true, true) if count("message") == 0 && count("messages") > 0 => SourceSchema::Legacy,
            (true, _) => SourceSchema::Modern,
            (false, true) => SourceSchema::Legacy,
            (false, false) => {
                let mut names: Vec<&String> = tables.keys().collect();
                names.sort();
                tracing::warn!(tables = ?names, "no message schema recognized");
                return Err(Error::NotMsgstore);
            }
        };
        // Diagnostics: only WhatsApp table names and counts.
        let main: Vec<(&str, i64)> = [
            "message", "messages", "chat", "chat_list", "jid", "jid_map", "message_media", "message_quoted",
            "message_add_on", "message_vcard", "message_location", "message_thumbnail", "message_system",
        ]
        .into_iter()
        .filter(|t| tables.contains_key(*t))
        .map(|t| (t, count(t)))
        .collect();
        tracing::info!(schema = kind.as_str(), tables = tables.len(), counts = ?main, "msgstore recognized");
        let features = [
            ("thumbnails", has("message_thumbnail", "message_row_id") || has("media_hash_thumbnail", "media_hash") || has("message_thumbnails", "key_id")),
            ("edits", has("message_edit_info", "message_row_id") || has("messages", "edit_version")),
            ("quotes", has("message_quoted", "message_row_id") || has("messages", "quoted_row_id")),
            ("reactions", has("message_add_on_reaction", "reaction")),
            ("vcards", has("message_vcard", "vcard") || has("messages_vcards", "vcard")),
            ("lid_to_phone", has("jid_map", "lid_row_id")),
            ("system_messages", tables.contains_key("message_system")),
            ("media", has("message_media", "message_row_id") || has("messages", "media_wa_type")),
        ];
        tracing::info!(features = ?features, "features found in the msgstore");
        let mut all: Vec<&String> = tables.keys().collect();
        all.sort();
        tracing::debug!(tables = ?all, "msgstore tables");
        Ok(Self { kind, tables })
    }

    pub fn has_col(&self, table: &str, col: &str) -> bool {
        self.tables.get(table).is_some_and(|c| c.contains(col))
    }

    /// `alias.col`, if the column exists. Otherwise, `NULL`.
    pub fn col(&self, table: &str, alias: &str, col: &str) -> String {
        if self.has_col(table, col) {
            format!("{alias}.{col}")
        } else {
            tracing::debug!(table = table, column = col, table_exists = self.tables.contains_key(table), "column missing from the msgstore (becomes NULL)");
            "NULL".to_owned()
        }
    }

    /// `LEFT JOIN table alias ON cond`, if the table exists (and has `required_col`).
    pub fn left_join(&self, table: &str, alias: &str, required_col: &str, cond: &str) -> String {
        if self.has_col(table, required_col) {
            format!("LEFT JOIN {table} {alias} ON {cond}")
        } else {
            tracing::debug!(table = table, column = required_col, "table/column missing from the msgstore (join skipped)");
            String::new()
        }
    }

    /// Columns of a LEFT JOIN can only be selected if the join exists.
    pub fn joined_col(&self, table: &str, join_col: &str, alias: &str, col: &str) -> String {
        if self.has_col(table, join_col) {
            self.col(table, alias, col)
        } else {
            "NULL".to_owned()
        }
    }
}
