//! Viewer queries over a vault database.
//!
//! * Cursor pagination on `(sent_at, id)`, always through the
//!   `message_by_chat(chat_id, sent_at, id)` index: opening the chat at the end or
//!   jumping to the middle of 1M messages costs the same.
//! * Pages come back in ascending chronological order.
//! * Message text is untrusted content and goes out as is. Whoever renders it
//!   is responsible for making sure it becomes text, never HTML.
//! * The search snippet marks the matched range with `\u{1}` … `\u{2}`
//!   (instead of HTML), so the UI can build the highlight with text nodes.

use std::collections::HashMap;

use rusqlite::{Connection, OptionalExtension, Row, params, params_from_iter};
use serde::{Deserialize, Serialize};

use crate::Error;

/// Maximum items per page, so a webview request cannot pull the whole database.
pub const MAX_PAGE: u32 = 500;

pub const HIGHLIGHT_START: char = '\u{1}';
pub const HIGHLIGHT_END: char = '\u{2}';

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Cursor {
    pub sent_at: i64,
    pub id: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Direction {
    /// Messages older than the cursor (no cursor: the chat's latest ones).
    Before,
    /// Messages newer than the cursor (no cursor: the chat's first ones).
    After,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum Anchor {
    Message { id: i64 },
    /// First message from this instant (ms) on. If there is none, the last one.
    Time { at: i64 },
}

// ---------------------------------------------------------------- chats

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Chat {
    pub id: i64,
    /// `direct`, `group`, `broadcast` or `channel`.
    pub kind: String,
    pub title: Option<String>,
    pub jid: String,
    pub phone: Option<String>,
    pub name: Option<String>,
    pub archived: bool,
    pub message_count: i64,
    pub last_message_at: Option<i64>,
    pub last: Option<LastMessage>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LastMessage {
    pub kind: String,
    /// Up to 120 characters.
    pub text: Option<String>,
    pub from_me: bool,
    pub sender: Option<Person>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Person {
    pub jid: String,
    pub phone: Option<String>,
    pub name: Option<String>,
}

/// All chats with messages, from most recent to oldest.
pub fn chats(conn: &Connection) -> Result<Vec<Chat>, Error> {
    let mut stmt = conn.prepare_cached(
        "SELECT c.id, c.kind, c.title, ct.jid, ct.phone, ct.name, c.archived, c.message_count,
                c.last_message_at, lm.kind, substr(lm.text, 1, 120), lm.from_me,
                s.jid, s.phone, s.name
         FROM chat c
         JOIN contact ct ON ct.id = c.contact_id
         LEFT JOIN message lm ON lm.id = (
             SELECT id FROM message WHERE chat_id = c.id ORDER BY sent_at DESC, id DESC LIMIT 1)
         LEFT JOIN contact s ON s.id = lm.sender_id
         WHERE c.message_count > 0
         ORDER BY c.last_message_at DESC, c.id DESC",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(Chat {
            id: r.get(0)?,
            kind: r.get(1)?,
            title: r.get(2)?,
            jid: r.get(3)?,
            phone: r.get(4)?,
            name: r.get(5)?,
            archived: r.get(6)?,
            message_count: r.get(7)?,
            last_message_at: r.get(8)?,
            last: match r.get::<_, Option<String>>(9)? {
                Some(kind) => Some(LastMessage {
                    kind,
                    text: r.get(10)?,
                    from_me: r.get(11)?,
                    sender: person(r, 12)?,
                }),
                None => None,
            },
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

// ---------------------------------------------------------------- messages

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub id: i64,
    pub chat_id: i64,
    pub sent_at: i64,
    pub from_me: bool,
    pub kind: String,
    pub text: Option<String>,
    /// Author, in groups (received messages).
    pub sender: Option<Person>,
    pub starred: bool,
    pub edited: bool,
    pub system_action: Option<i64>,
    pub quoted: Option<Quoted>,
    pub media: Option<Media>,
    pub location: Option<Location>,
    pub contacts: Vec<ContactCard>,
    pub reactions: Vec<Reaction>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Quoted {
    /// Original message, when it is in the database (to jump to it).
    pub message_id: Option<i64>,
    pub text: Option<String>,
    pub from_me: bool,
    pub sender: Option<Person>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Media {
    pub mime: Option<String>,
    pub size: Option<i64>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub duration_s: Option<i64>,
    pub file_name: Option<String>,
    pub transcription: Option<String>,
    pub has_thumbnail: bool,
    /// The file is in the vault (filled in by whoever queries the media index).
    pub available: bool,
    /// SHA-256 of the file (base64), as WhatsApp stores it. Not sent to the UI.
    #[serde(skip)]
    pub file_hash: Option<String>,
    /// Path on the phone. Not sent to the UI.
    #[serde(skip)]
    pub file_path: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Location {
    pub latitude: f64,
    pub longitude: f64,
    pub name: Option<String>,
    pub address: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContactCard {
    pub name: Option<String>,
    pub phones: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Reaction {
    pub emoji: String,
    pub count: u32,
    /// One of the reactions is yours.
    pub mine: bool,
}

const MESSAGE_SELECT: &str = "
    SELECT m.id, m.chat_id, m.sent_at, m.from_me, m.kind, m.text, m.starred, m.edited, m.system_action,
           s.jid, s.phone, s.name,
           m.quoted_key_id, m.quoted_message_id, m.quoted_text, m.quoted_from_me, qs.jid, qs.phone, qs.name,
           md.message_id, md.mime, md.size, md.width, md.height, md.duration_s, md.file_name, md.transcription,
           EXISTS(SELECT 1 FROM thumbnail t WHERE t.message_id = m.id),
           l.latitude, l.longitude, l.name, l.address, md.file_hash, md.file_path
    FROM message m
    LEFT JOIN contact s  ON s.id = m.sender_id
    LEFT JOIN contact qs ON qs.id = m.quoted_sender_id
    LEFT JOIN media md   ON md.message_id = m.id
    LEFT JOIN location l ON l.message_id = m.id";

fn person(r: &Row, at: usize) -> rusqlite::Result<Option<Person>> {
    Ok(r.get::<_, Option<String>>(at)?.map(|jid| Person {
        jid,
        phone: r.get(at + 1).ok().flatten(),
        name: r.get(at + 2).ok().flatten(),
    }))
}

fn message_row(r: &Row) -> rusqlite::Result<Message> {
    let quoted = if r.get::<_, Option<String>>(12)?.is_some() || r.get::<_, Option<i64>>(13)?.is_some() {
        Some(Quoted {
            message_id: r.get(13)?,
            text: r.get(14)?,
            from_me: r.get::<_, Option<bool>>(15)?.unwrap_or(false),
            sender: person(r, 16)?,
        })
    } else {
        None
    };
    let media = match r.get::<_, Option<i64>>(19)? {
        Some(_) => Some(Media {
            mime: r.get(20)?,
            size: r.get(21)?,
            width: r.get(22)?,
            height: r.get(23)?,
            duration_s: r.get(24)?,
            file_name: r.get(25)?,
            transcription: r.get(26)?,
            has_thumbnail: r.get(27)?,
            available: false,
            file_hash: r.get(32)?,
            file_path: r.get(33)?,
        }),
        // Stickers and the like with no row in `media` may still have a thumbnail.
        None if r.get::<_, bool>(27)? => Some(Media {
            mime: None,
            size: None,
            width: None,
            height: None,
            duration_s: None,
            file_name: None,
            transcription: None,
            has_thumbnail: true,
            available: false,
            file_hash: None,
            file_path: None,
        }),
        None => None,
    };
    let location = match (r.get::<_, Option<f64>>(28)?, r.get::<_, Option<f64>>(29)?) {
        (Some(latitude), Some(longitude)) => Some(Location {
            latitude,
            longitude,
            name: r.get(30)?,
            address: r.get(31)?,
        }),
        _ => None,
    };
    Ok(Message {
        id: r.get(0)?,
        chat_id: r.get(1)?,
        sent_at: r.get(2)?,
        from_me: r.get(3)?,
        kind: r.get(4)?,
        text: r.get(5)?,
        starred: r.get(6)?,
        edited: r.get(7)?,
        system_action: r.get(8)?,
        sender: person(r, 9)?,
        quoted,
        media,
        location,
        contacts: Vec::new(),
        reactions: Vec::new(),
    })
}

/// A page of a chat's messages, in chronological order.
pub fn messages(
    conn: &Connection,
    chat_id: i64,
    direction: Direction,
    cursor: Option<Cursor>,
    limit: u32,
) -> Result<Vec<Message>, Error> {
    let limit = limit.clamp(1, MAX_PAGE);
    let sql = match direction {
        Direction::Before => format!(
            "{MESSAGE_SELECT} WHERE m.chat_id = ?1 AND (m.sent_at, m.id) < (?2, ?3)
             ORDER BY m.sent_at DESC, m.id DESC LIMIT ?4"
        ),
        Direction::After => format!(
            "{MESSAGE_SELECT} WHERE m.chat_id = ?1 AND (m.sent_at, m.id) > (?2, ?3)
             ORDER BY m.sent_at ASC, m.id ASC LIMIT ?4"
        ),
    };
    let c = cursor.unwrap_or(match direction {
        Direction::Before => Cursor { sent_at: i64::MAX, id: i64::MAX },
        Direction::After => Cursor { sent_at: i64::MIN, id: i64::MIN },
    });
    let mut out: Vec<Message> = conn
        .prepare_cached(&sql)?
        .query_map(params![chat_id, c.sent_at, c.id, limit], message_row)?
        .collect::<Result<_, _>>()?;
    if direction == Direction::Before {
        out.reverse();
    }
    attach_extras(conn, &mut out)?;
    Ok(out)
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Around {
    pub messages: Vec<Message>,
    /// Index of the anchor in `messages`. `None` if the chat is empty.
    pub anchor_index: Option<usize>,
}

/// Messages around an anchor (search result, quote, date).
pub fn messages_around(conn: &Connection, chat_id: i64, anchor: Anchor, limit: u32) -> Result<Around, Error> {
    let limit = limit.clamp(2, MAX_PAGE);
    let anchor_row = match anchor {
        Anchor::Message { id } => conn
            .prepare_cached(&format!("{MESSAGE_SELECT} WHERE m.id = ?1 AND m.chat_id = ?2"))?
            .query_row(params![id, chat_id], message_row)
            .optional()?,
        Anchor::Time { at } => {
            let first = conn
                .prepare_cached(&format!(
                    "{MESSAGE_SELECT} WHERE m.chat_id = ?1 AND m.sent_at >= ?2
                     ORDER BY m.sent_at ASC, m.id ASC LIMIT 1"
                ))?
                .query_row(params![chat_id, at], message_row)
                .optional()?;
            match first {
                Some(m) => Some(m),
                None => messages(conn, chat_id, Direction::Before, None, 1)?.pop(),
            }
        }
    };
    let Some(mut anchor) = anchor_row else {
        return Ok(Around { messages: Vec::new(), anchor_index: None });
    };
    attach_extras(conn, std::slice::from_mut(&mut anchor))?;

    let cursor = Some(Cursor { sent_at: anchor.sent_at, id: anchor.id });
    let mut out = messages(conn, chat_id, Direction::Before, cursor, limit / 2)?;
    let anchor_index = out.len();
    out.push(anchor);
    out.extend(messages(conn, chat_id, Direction::After, cursor, limit / 2)?);
    Ok(Around { messages: out, anchor_index: Some(anchor_index) })
}

/// Reactions and contact cards for the page, in two queries.
fn attach_extras(conn: &Connection, page: &mut [Message]) -> Result<(), Error> {
    if page.is_empty() {
        return Ok(());
    }
    let ids: Vec<i64> = page.iter().map(|m| m.id).collect();
    let placeholders = vec!["?"; ids.len()].join(",");
    let index: HashMap<i64, usize> = ids.iter().enumerate().map(|(i, id)| (*id, i)).collect();

    let mut stmt = conn.prepare(&format!(
        "SELECT message_id, emoji, count(*), max(from_me) FROM reaction
         WHERE message_id IN ({placeholders}) GROUP BY message_id, emoji ORDER BY min(id)"
    ))?;
    let mut rows = stmt.query(params_from_iter(&ids))?;
    while let Some(r) = rows.next()? {
        let i = index[&r.get::<_, i64>(0)?];
        page[i].reactions.push(Reaction { emoji: r.get(1)?, count: r.get(2)?, mine: r.get(3)? });
    }

    let mut stmt = conn.prepare(&format!(
        "SELECT message_id, data FROM vcard WHERE message_id IN ({placeholders}) ORDER BY id"
    ))?;
    let mut rows = stmt.query(params_from_iter(&ids))?;
    while let Some(r) = rows.next()? {
        let i = index[&r.get::<_, i64>(0)?];
        page[i].contacts.push(parse_vcard(&r.get::<_, String>(1)?));
    }
    Ok(())
}

/// Extracts the name and phone numbers from a vCard. Only what is needed for display:
/// the full vCard never goes to the UI.
pub fn parse_vcard(data: &str) -> ContactCard {
    let mut card = ContactCard { name: None, phones: Vec::new() };
    for line in data.lines() {
        let line = line.trim_end_matches('\r');
        let Some((field, value)) = line.split_once(':') else { continue };
        let field = field.split(';').next().unwrap_or("").to_ascii_uppercase();
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        match field.as_str() {
            "FN" if card.name.is_none() => card.name = Some(value.to_owned()),
            // `TEL;type=CELL;waid=5511...:+55 11 ...`: the number comes after the last `:`.
            "TEL" => card.phones.push(line.rsplit(':').next().unwrap_or(value).trim().to_owned()),
            _ => {}
        }
    }
    card
}

// ---------------------------------------------------------------- search

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub message_id: i64,
    pub chat_id: i64,
    pub sent_at: i64,
    pub from_me: bool,
    pub sender: Option<Person>,
    /// Excerpt with the terms between `\u{1}` and `\u{2}`.
    pub snippet: String,
}

/// Temporary `fts5vocab` table, created on the viewer's connection
/// (see [`crate::StoreHandle::open_read_only`]).
pub(crate) const VOCAB_DDL: &str =
    "CREATE VIRTUAL TABLE IF NOT EXISTS temp.message_vocab USING fts5vocab(main, message_fts, row);";

/// Prefixes shorter than this are not expanded (they match the exact word).
const MIN_PREFIX_CHARS: usize = 3;
/// Maximum vocabulary words per prefix.
const MAX_PREFIX_TERMS: usize = 64;

/// Typed words, without punctuation at the ends. FTS5 operators (`OR`,
/// `NEAR`, `-`, quotes) become plain text because each word is quoted.
fn terms(input: &str) -> Vec<&str> {
    input
        .split_whitespace()
        .map(|t| t.trim_matches(|c: char| !c.is_alphanumeric()))
        .filter(|t| !t.is_empty())
        .collect()
}

fn quoted(term: &str) -> String {
    format!("\"{}\"", term.replace('"', "\"\""))
}

/// Builds the FTS5 expression. The earlier words match exactly and the last one
/// by prefix (the user may still be typing).
///
/// FTS5's native prefix (`"men"*`) builds the list of all occurrences in memory
/// before returning the first one: ~200 ms for a common term in
/// 300k messages. So the prefix is expanded into the index's real words
/// (`"mensagem" OR "mensagens"`), which FTS5 walks on demand.
pub fn fts_query(conn: &Connection, input: &str) -> Result<Option<String>, Error> {
    let terms = terms(input);
    let Some((last, init)) = terms.split_last() else { return Ok(None) };
    let mut parts: Vec<String> = init.iter().map(|t| quoted(t)).collect();
    parts.push(prefix_expr(conn, last)?);
    Ok(Some(parts.join(" ")))
}

fn prefix_expr(conn: &Connection, term: &str) -> Result<String, Error> {
    let folded = fold(term);
    let simple = folded.chars().all(char::is_alphanumeric);
    if !simple || folded.chars().count() < MIN_PREFIX_CHARS {
        return Ok(if simple { quoted(term) } else { format!("{}*", quoted(term)) });
    }
    let mut upper = folded.clone();
    upper.push(char::MAX);
    let words: Vec<String> = conn
        .prepare_cached("SELECT term FROM temp.message_vocab WHERE term >= ?1 AND term < ?2 LIMIT ?3")?
        .query_map(params![folded, upper, MAX_PREFIX_TERMS as i64], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    Ok(match words.len() {
        // The diacritic folding here may diverge from the tokenizer for less
        // common scripts: in that case, the native prefix guarantees the result.
        0 => format!("{}*", quoted(term)),
        1 => quoted(&words[0]),
        _ => format!("({})", words.iter().map(|w| quoted(w)).collect::<Vec<_>>().join(" OR ")),
    })
}

/// Lowercase and without diacritics, like the `unicode61 remove_diacritics 2` tokenizer
/// (for the Latin alphabet; everything else is only lowercased).
pub fn fold(s: &str) -> String {
    s.chars().flat_map(char::to_lowercase).map(fold_char).collect()
}

fn fold_char(c: char) -> char {
    match c {
        'à'..='å' | 'ā' | 'ă' | 'ą' => 'a',
        'ç' | 'ć' | 'ĉ' | 'ċ' | 'č' => 'c',
        'è'..='ë' | 'ē' | 'ĕ' | 'ė' | 'ę' | 'ě' => 'e',
        'ì'..='ï' | 'ĩ' | 'ī' | 'ĭ' | 'į' => 'i',
        'ñ' | 'ń' | 'ņ' | 'ň' => 'n',
        'ò'..='ö' | 'ø' | 'ō' | 'ŏ' | 'ő' => 'o',
        'ù'..='ü' | 'ũ' | 'ū' | 'ŭ' | 'ů' | 'ű' | 'ų' => 'u',
        'ý' | 'ÿ' => 'y',
        _ => c,
    }
}

/// Search ignoring diacritics and case, from most recent to
/// oldest. `before` continues from the last result of the previous
/// page (only the `id` matters, since it follows chronological order).
pub fn search(
    conn: &Connection,
    input: &str,
    chat_id: Option<i64>,
    before: Option<Cursor>,
    limit: u32,
) -> Result<Vec<SearchHit>, Error> {
    let Some(query) = fts_query(conn, input)? else { return Ok(Vec::new()) };
    let limit = limit.clamp(1, MAX_PAGE);
    let before_id = before.map_or(i64::MAX, |c| c.id);
    let highlight: Vec<String> = terms(input).iter().map(|t| fold(t)).collect();

    // The importer writes messages in chronological order, so descending
    // `rowid` = most recent first: FTS5 yields in that order and stops
    // at the LIMIT, without sorting all results.
    let hits = conn
        .prepare_cached(
            "SELECT m.id, m.chat_id, m.sent_at, m.from_me, s.jid, s.phone, s.name, m.text
             FROM message_fts
             JOIN message m ON m.id = message_fts.rowid
             LEFT JOIN contact s ON s.id = m.sender_id
             WHERE message_fts MATCH ?1 AND message_fts.rowid < ?3
               AND (?2 IS NULL OR m.chat_id = ?2)
             ORDER BY message_fts.rowid DESC LIMIT ?4",
        )?
        .query_map(params![query, chat_id, before_id, limit], |r| {
            Ok(SearchHit {
                message_id: r.get(0)?,
                chat_id: r.get(1)?,
                sent_at: r.get(2)?,
                from_me: r.get(3)?,
                sender: person(r, 4)?,
                snippet: snippet(&r.get::<_, Option<String>>(7)?.unwrap_or_default(), &highlight),
            })
        })?
        .collect::<Result<_, _>>()?;
    Ok(hits)
}

/// Excerpt of up to ~16 words around the first occurrence, with every
/// word starting with one of the terms between `\u{1}` and `\u{2}`.
pub fn snippet(text: &str, terms: &[String]) -> String {
    const BEFORE: usize = 5;
    const WORDS: usize = 16;

    // Words = alphanumeric runs (like the tokenizer), as byte offsets.
    let mut words: Vec<(usize, usize)> = Vec::new();
    let mut start = None;
    for (i, c) in text.char_indices() {
        match (c.is_alphanumeric(), start) {
            (true, None) => start = Some(i),
            (false, Some(s)) => {
                words.push((s, i));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        words.push((s, text.len()));
    }
    if words.is_empty() {
        return text.chars().take(120).collect();
    }

    let hit = |&(s, e): &(usize, usize)| {
        let w = fold(&text[s..e]);
        terms.iter().any(|t| !t.is_empty() && w.starts_with(t.as_str()))
    };
    let first = words.iter().position(hit).unwrap_or(0);
    let from = first.saturating_sub(BEFORE);
    let to = (from + WORDS).min(words.len());

    let mut out = String::new();
    if from > 0 {
        out.push('…');
    }
    let mut cursor = words[from].0;
    for w in &words[from..to] {
        out.push_str(&text[cursor..w.0]);
        if hit(w) {
            out.push(HIGHLIGHT_START);
            out.push_str(&text[w.0..w.1]);
            out.push(HIGHLIGHT_END);
        } else {
            out.push_str(&text[w.0..w.1]);
        }
        cursor = w.1;
    }
    if to < words.len() {
        out.push('…');
    }
    // Line breaks become spaces: the excerpt is shown on one line.
    out.replace(['\n', '\r'], " ")
}

// ---------------------------------------------------------------- media

/// Reference to a media item in the msgstore, to find the file in the vault.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MediaRef {
    pub message_id: i64,
    pub kind: String,
    pub file_hash: Option<String>,
    pub file_path: Option<String>,
    pub mime: Option<String>,
    pub file_name: Option<String>,
}

const MEDIA_REF_SELECT: &str = "
    SELECT m.id, m.kind, md.file_hash, md.file_path, md.mime, md.file_name
    FROM media md JOIN message m ON m.id = md.message_id";

fn media_ref_row(r: &Row) -> rusqlite::Result<MediaRef> {
    Ok(MediaRef {
        message_id: r.get(0)?,
        kind: r.get(1)?,
        file_hash: r.get(2)?,
        file_path: r.get(3)?,
        mime: r.get(4)?,
        file_name: r.get(5)?,
    })
}

pub fn media_ref(conn: &Connection, message_id: i64) -> Result<Option<MediaRef>, Error> {
    Ok(conn
        .prepare_cached(&format!("{MEDIA_REF_SELECT} WHERE md.message_id = ?1"))?
        .query_row([message_id], media_ref_row)
        .optional()?)
}

/// All media with a hash or path (to choose what to import).
/// Deleted messages are left out.
pub fn media_refs(conn: &Connection) -> Result<Vec<MediaRef>, Error> {
    Ok(conn
        .prepare(&format!(
            "{MEDIA_REF_SELECT} WHERE m.kind <> 'deleted' AND (md.file_hash IS NOT NULL OR md.file_path IS NOT NULL)"
        ))?
        .query_map([], media_ref_row)?
        .collect::<Result<_, _>>()?)
}

/// A message's thumbnail (usually a JPEG of a few KB).
pub fn thumbnail(conn: &Connection, message_id: i64) -> Result<Option<Vec<u8>>, Error> {
    Ok(conn
        .prepare_cached("SELECT data FROM thumbnail WHERE message_id = ?1")?
        .query_row([message_id], |r| r.get(0))
        .optional()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn_with_words(words: &[&str]) -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE VIRTUAL TABLE message_fts USING fts5(text, tokenize = 'unicode61 remove_diacritics 2');",
        )
        .unwrap();
        conn.execute_batch(VOCAB_DDL).unwrap();
        for w in words {
            conn.execute("INSERT INTO message_fts(text) VALUES (?1)", [w]).unwrap();
        }
        conn
    }

    #[test]
    fn fts_query_neutralizes_operators() {
        let conn = conn_with_words(&[]);
        let q = |s: &str| fts_query(&conn, s).unwrap();
        assert_eq!(q("  "), None);
        assert_eq!(q("?!"), None);
        // No words in the index starting with the prefix: native prefix.
        assert_eq!(q("café"), Some("\"café\"*".into()));
        // Short prefix: exact word.
        assert_eq!(q("a OR b"), Some("\"a\" \"OR\" \"b\"".into()));
        assert_eq!(q("\"x\" -y NEAR(z)"), Some("\"x\" \"y\" \"NEAR(z\"*".into()));
    }

    #[test]
    fn prefix_expanded_from_vocabulary() {
        let conn = conn_with_words(&["Mensagem", "mensagens", "menos", "Café", "cafeteira"]);
        let q = |s: &str| fts_query(&conn, s).unwrap().unwrap();
        assert_eq!(q("mensa"), "(\"mensagem\" OR \"mensagens\")");
        assert_eq!(q("MENO"), "\"menos\"");
        // Diacritic in the typed text: the vocabulary lookup uses the unaccented form.
        assert_eq!(q("cafê"), "(\"cafe\" OR \"cafeteira\")");
        assert_eq!(q("menos mensa"), "\"menos\" (\"mensagem\" OR \"mensagens\")");
    }

    #[test]
    fn snippet_with_highlight() {
        let s = snippet("Oi, tudo bem? Vamos tomar um café\namanhã", &["cafe".into()]);
        assert_eq!(s, "…tudo bem? Vamos tomar um \u{1}café\u{2} amanhã");

        let long = (1..=40).map(|i| format!("p{i}")).collect::<Vec<_>>().join(" ");
        let s = snippet(&long, &["p20".into()]);
        assert!(s.starts_with("…p15 ") && s.ends_with(" p30…"), "{s}");
        assert!(s.contains("\u{1}p20\u{2}"));

        assert_eq!(snippet("", &["x".into()]), "");
        assert_eq!(snippet("no term", &["zzz".into()]), "no term");
    }

    #[test]
    fn vcard() {
        let card = parse_vcard(
            "BEGIN:VCARD\r\nVERSION:3.0\r\nN:;Zé;;;\r\nFN:Zé da Silva\r\n\
             TEL;type=CELL;waid=5511955555555:+55 11 95555-5555\r\nEND:VCARD",
        );
        assert_eq!(card.name.as_deref(), Some("Zé da Silva"));
        assert_eq!(card.phones, ["+55 11 95555-5555"]);
    }
}
