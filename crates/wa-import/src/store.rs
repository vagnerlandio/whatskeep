//! The app's normalized schema: what the viewer reads, regardless of the WhatsApp version.
//!
//! Dates in milliseconds since the Unix epoch (UTC). Text and caption live in
//! `message.text`, indexed by FTS5 ignoring diacritics.

use std::collections::HashMap;

use rusqlite::{params, Connection, OptionalExtension};

use crate::jid::{phone_of, ChatKind};
use crate::{Error, MessageKind};

pub const SCHEMA_VERSION: i64 = 1;

const DDL: &str = r#"
CREATE TABLE IF NOT EXISTS meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
) WITHOUT ROWID;

-- People and groups. `jid` is the WhatsApp identifier, already resolved from LID
-- to phone number when possible. `name` stays empty: the msgstore has no address book.
CREATE TABLE IF NOT EXISTS contact (
    id    INTEGER PRIMARY KEY,
    jid   TEXT NOT NULL UNIQUE,
    phone TEXT,
    name  TEXT
);

CREATE TABLE IF NOT EXISTS chat (
    id              INTEGER PRIMARY KEY,
    contact_id      INTEGER NOT NULL REFERENCES contact(id),
    kind            TEXT NOT NULL,          -- direct | group | broadcast | channel
    title           TEXT,                   -- group/channel name
    archived        INTEGER NOT NULL DEFAULT 0,
    hidden          INTEGER NOT NULL DEFAULT 0,
    message_count   INTEGER NOT NULL DEFAULT 0,
    last_message_at INTEGER
);

-- Messages are inserted in chronological order (sent_at, source id), so
-- `id` grows with `sent_at`. Search relies on this to scan the FTS by descending
-- rowid and stop at the limit, without sorting all results.
CREATE TABLE IF NOT EXISTS message (
    id                INTEGER PRIMARY KEY,
    chat_id           INTEGER NOT NULL REFERENCES chat(id),
    key_id            TEXT,
    from_me           INTEGER NOT NULL,
    sender_id         INTEGER REFERENCES contact(id),  -- author, in groups
    sent_at           INTEGER NOT NULL,
    kind              TEXT NOT NULL,
    wa_type           INTEGER,                         -- original code (diagnostics)
    text              TEXT,                            -- text or caption
    starred           INTEGER NOT NULL DEFAULT 0,
    edited            INTEGER NOT NULL DEFAULT 0,
    system_action     INTEGER,
    quoted_key_id     TEXT,
    quoted_message_id INTEGER REFERENCES message(id),
    quoted_text       TEXT,
    quoted_from_me    INTEGER,
    quoted_sender_id  INTEGER REFERENCES contact(id)
);
CREATE INDEX IF NOT EXISTS message_by_chat ON message(chat_id, sent_at, id);
CREATE INDEX IF NOT EXISTS message_by_key  ON message(chat_id, key_id);

CREATE TABLE IF NOT EXISTS media (
    message_id    INTEGER PRIMARY KEY REFERENCES message(id),
    file_path     TEXT,     -- relative path on the phone/backup
    mime          TEXT,
    size          INTEGER,
    width         INTEGER,
    height        INTEGER,
    duration_s    INTEGER,
    file_name     TEXT,
    file_hash     TEXT,
    transcription TEXT
);

CREATE TABLE IF NOT EXISTS thumbnail (
    message_id INTEGER PRIMARY KEY REFERENCES message(id),
    data       BLOB NOT NULL
);

CREATE TABLE IF NOT EXISTS location (
    message_id INTEGER PRIMARY KEY REFERENCES message(id),
    latitude   REAL NOT NULL,
    longitude  REAL NOT NULL,
    name       TEXT,
    address    TEXT
);

CREATE TABLE IF NOT EXISTS vcard (
    id         INTEGER PRIMARY KEY,
    message_id INTEGER NOT NULL REFERENCES message(id),
    data       TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS vcard_by_message ON vcard(message_id);

CREATE TABLE IF NOT EXISTS reaction (
    id         INTEGER PRIMARY KEY,
    message_id INTEGER NOT NULL REFERENCES message(id),
    from_me    INTEGER NOT NULL,
    sender_id  INTEGER REFERENCES contact(id),
    emoji      TEXT NOT NULL,
    reacted_at INTEGER
);
CREATE INDEX IF NOT EXISTS reaction_by_message ON reaction(message_id);

CREATE VIRTUAL TABLE IF NOT EXISTS message_fts USING fts5(
    text,
    content = 'message',
    content_rowid = 'id',
    tokenize = 'unicode61 remove_diacritics 2'
);
"#;

pub(crate) fn create_schema(conn: &Connection) -> Result<(), Error> {
    conn.execute_batch(DDL)?;
    Ok(())
}

pub(crate) fn has_data(conn: &Connection) -> Result<bool, Error> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM chat) OR EXISTS(SELECT 1 FROM message)",
        [],
        |r| r.get(0),
    )?)
}

/// Steps that depend on all messages already being written.
pub(crate) fn finish(conn: &Connection, source_app: &str, source_schema: &str) -> Result<(), Error> {
    conn.execute_batch(
        r#"
        UPDATE message SET quoted_message_id = (
            SELECT o.id FROM message o
            WHERE o.chat_id = message.chat_id AND o.key_id = message.quoted_key_id AND o.id <> message.id
            LIMIT 1
        ) WHERE quoted_key_id IS NOT NULL;

        UPDATE chat SET
            message_count   = (SELECT count(*)     FROM message m WHERE m.chat_id = chat.id),
            last_message_at = (SELECT max(sent_at) FROM message m WHERE m.chat_id = chat.id);

        INSERT INTO message_fts(message_fts) VALUES ('rebuild');
        "#,
    )?;
    let mut meta = conn.prepare("INSERT OR REPLACE INTO meta(key, value) VALUES (?1, ?2)")?;
    meta.execute(params!["schema_version", SCHEMA_VERSION.to_string()])?;
    meta.execute(params!["source_app", source_app])?;
    meta.execute(params!["source_schema", source_schema])?;
    meta.execute(params![
        "imported_at",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis().to_string())
            .unwrap_or_default()
    ])?;
    Ok(())
}

pub(crate) struct NewMessage<'a> {
    pub chat_id: i64,
    pub key_id: Option<&'a str>,
    pub from_me: bool,
    pub sender_id: Option<i64>,
    pub sent_at: i64,
    pub kind: MessageKind,
    pub wa_type: i64,
    pub text: Option<&'a str>,
    pub starred: bool,
    pub edited: bool,
    pub system_action: Option<i64>,
    pub quoted_key_id: Option<&'a str>,
    pub quoted_text: Option<&'a str>,
    pub quoted_from_me: Option<bool>,
    pub quoted_sender_id: Option<i64>,
}

#[derive(Default)]
pub(crate) struct NewMedia<'a> {
    pub file_path: Option<&'a str>,
    pub mime: Option<&'a str>,
    pub size: Option<i64>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub duration_s: Option<i64>,
    pub file_name: Option<&'a str>,
    pub file_hash: Option<&'a str>,
    pub transcription: Option<&'a str>,
}

/// Writes to the app's schema, with a per-JID contact cache.
pub(crate) struct Writer<'c> {
    conn: &'c Connection,
    contacts: HashMap<String, i64>,
}

impl<'c> Writer<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        Self {
            conn,
            contacts: HashMap::new(),
        }
    }

    pub fn contact(&mut self, jid: &str) -> Result<i64, Error> {
        if let Some(&id) = self.contacts.get(jid) {
            return Ok(id);
        }
        let existing: Option<i64> = self
            .conn
            .prepare_cached("SELECT id FROM contact WHERE jid = ?1")?
            .query_row([jid], |r| r.get(0))
            .optional()?;
        let id = match existing {
            Some(id) => id,
            None => {
                self.conn
                    .prepare_cached("INSERT INTO contact(jid, phone) VALUES (?1, ?2)")?
                    .execute(params![jid, phone_of(jid)])?;
                self.conn.last_insert_rowid()
            }
        };
        self.contacts.insert(jid.to_owned(), id);
        Ok(id)
    }

    pub fn chat(
        &mut self,
        jid: &str,
        kind: ChatKind,
        title: Option<&str>,
        archived: bool,
        hidden: bool,
    ) -> Result<i64, Error> {
        let contact_id = self.contact(jid)?;
        // In one-to-one chats the `subject` is usually empty or garbage.
        let title = match kind {
            ChatKind::Direct => None,
            _ => title.filter(|t| !t.trim().is_empty()),
        };
        self.conn
            .prepare_cached("INSERT INTO chat(contact_id, kind, title, archived, hidden) VALUES (?1, ?2, ?3, ?4, ?5)")?
            .execute(params![contact_id, kind.as_str(), title, archived, hidden])?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn message(&mut self, m: &NewMessage) -> Result<i64, Error> {
        self.conn
            .prepare_cached(
                "INSERT INTO message(chat_id, key_id, from_me, sender_id, sent_at, kind, wa_type, text,
                    starred, edited, system_action, quoted_key_id, quoted_text, quoted_from_me, quoted_sender_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            )?
            .execute(params![
                m.chat_id,
                m.key_id,
                m.from_me,
                m.sender_id,
                m.sent_at,
                m.kind.as_str(),
                m.wa_type,
                m.text.filter(|t| !t.is_empty()),
                m.starred,
                m.edited,
                m.system_action,
                m.quoted_key_id,
                m.quoted_text,
                m.quoted_from_me,
                m.quoted_sender_id,
            ])?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn media(&mut self, message_id: i64, m: &NewMedia) -> Result<(), Error> {
        self.conn
            .prepare_cached(
                "INSERT INTO media(message_id, file_path, mime, size, width, height, duration_s, file_name, file_hash, transcription)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            )?
            .execute(params![
                message_id,
                m.file_path,
                m.mime,
                m.size,
                m.width.filter(|&w| w > 0),
                m.height.filter(|&h| h > 0),
                m.duration_s,
                m.file_name,
                m.file_hash,
                m.transcription.filter(|t| !t.is_empty()),
            ])?;
        Ok(())
    }

    pub fn thumbnail(&mut self, message_id: i64, data: &[u8]) -> Result<(), Error> {
        if data.is_empty() {
            return Ok(());
        }
        self.conn
            .prepare_cached("INSERT OR REPLACE INTO thumbnail(message_id, data) VALUES (?1, ?2)")?
            .execute(params![message_id, data])?;
        Ok(())
    }

    pub fn location(
        &mut self,
        message_id: i64,
        lat: f64,
        lon: f64,
        name: Option<&str>,
        address: Option<&str>,
    ) -> Result<(), Error> {
        self.conn
            .prepare_cached("INSERT OR REPLACE INTO location(message_id, latitude, longitude, name, address) VALUES (?1, ?2, ?3, ?4, ?5)")?
            .execute(params![message_id, lat, lon, name, address])?;
        Ok(())
    }

    pub fn vcard(&mut self, message_id: i64, data: &str) -> Result<(), Error> {
        self.conn
            .prepare_cached("INSERT INTO vcard(message_id, data) VALUES (?1, ?2)")?
            .execute(params![message_id, data])?;
        Ok(())
    }

    pub fn reaction(
        &mut self,
        message_id: i64,
        from_me: bool,
        sender_id: Option<i64>,
        emoji: &str,
        reacted_at: Option<i64>,
    ) -> Result<(), Error> {
        self.conn
            .prepare_cached("INSERT INTO reaction(message_id, from_me, sender_id, emoji, reacted_at) VALUES (?1, ?2, ?3, ?4, ?5)")?
            .execute(params![message_id, from_me, sender_id, emoji, reacted_at])?;
        Ok(())
    }
}
