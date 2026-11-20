//! Diagnostics for `message_type` values the importer does not know yet.
//!
//! For each unknown type it logs which WhatsApp tables point at those messages,
//! which `message` columns they fill and the MIME families of their media, so a
//! real backup tells us what the type is. Only table/column names, MIME
//! families and counts reach the log: never content.

use std::collections::{BTreeMap, BTreeSet};

use rusqlite::Connection;

use crate::source::SourceInfo;

/// What points at the messages of one unknown type.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct TypeProbe {
    /// Table (`table` for `message_row_id`, `table^` for `parent_message_row_id`) → messages.
    pub tables: BTreeMap<String, u64>,
    /// `message` column → messages where it is neither NULL, 0 nor ''.
    pub columns: BTreeMap<String, u64>,
    /// MIME family (`image`, `audio`, ... or `none`) → messages with a `message_media` row.
    pub media: BTreeMap<String, u64>,
    /// Messages with a `message_media` row that has a file path.
    pub media_with_file: u64,
}

/// Columns that say nothing about the type (every message has them).
const SKIP_COLUMNS: &[&str] = &["_id", "message_type", "chat_row_id", "key_id", "timestamp", "received_timestamp", "sort_id"];

pub(crate) fn unknown_types(src: &Connection, info: &SourceInfo, types: &BTreeMap<i64, u64>) -> BTreeMap<i64, TypeProbe> {
    let mut out: BTreeMap<i64, TypeProbe> = types.keys().map(|&t| (t, TypeProbe::default())).collect();
    if types.is_empty() || !info.has_col("message", "message_type") {
        return out;
    }
    let list = types.keys().map(i64::to_string).collect::<Vec<_>>().join(",");
    let in_list = format!("CAST(m.message_type AS INTEGER) IN ({list})");
    let quote = |name: &str| format!("\"{}\"", name.replace('"', "\"\""));

    // Tables that point at a message.
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for table in info.table_names() {
        if table == "message" {
            continue;
        }
        for (col, label) in [("message_row_id", table.to_owned()), ("parent_message_row_id", format!("{table}^"))] {
            if !info.has_col(table, col) {
                continue;
            }
            let sql = format!(
                "SELECT CAST(m.message_type AS INTEGER), count(DISTINCT m._id)
                 FROM {t} x JOIN message m ON m._id = x.{col}
                 WHERE {in_list} GROUP BY 1",
                t = quote(table),
            );
            let rows = query_pairs(src, &sql);
            for (t, n) in rows {
                if let Some(p) = out.get_mut(&t) {
                    p.tables.insert(label.clone(), n);
                    seen.insert(table);
                }
            }
        }
    }

    // `message` columns those messages fill.
    let cols: Vec<&str> = info.columns("message").into_iter().filter(|c| !SKIP_COLUMNS.contains(c)).collect();
    if !cols.is_empty() {
        let aggregates = cols
            .iter()
            .map(|c| format!("count(NULLIF(NULLIF(m.{}, 0), ''))", quote(c)))
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!("SELECT CAST(m.message_type AS INTEGER), {aggregates} FROM message m WHERE {in_list} GROUP BY 1");
        match src.prepare(&sql) {
            Ok(mut stmt) => {
                let result = stmt.query_map([], |r| {
                    let t: i64 = r.get(0)?;
                    let counts = (0..cols.len()).map(|i| r.get::<_, i64>(i + 1)).collect::<Result<Vec<_>, _>>()?;
                    Ok((t, counts))
                });
                if let Ok(rows) = result {
                    for (t, counts) in rows.flatten() {
                        if let Some(p) = out.get_mut(&t) {
                            for (c, n) in cols.iter().zip(counts) {
                                if n > 0 {
                                    p.columns.insert((*c).to_owned(), n as u64);
                                }
                            }
                        }
                    }
                }
            }
            Err(e) => tracing::debug!(error = %e, "probe: message columns query failed"),
        }
    }

    // MIME families of their media.
    if info.has_col("message_media", "message_row_id") && info.has_col("message_media", "mime_type") {
        let file = if info.has_col("message_media", "file_path") { "mm.file_path IS NOT NULL AND mm.file_path <> ''" } else { "0" };
        let sql = format!(
            "SELECT CAST(m.message_type AS INTEGER),
                    CASE WHEN instr(mm.mime_type, '/') > 1 THEN lower(substr(mm.mime_type, 1, instr(mm.mime_type, '/') - 1)) ELSE 'none' END,
                    count(*), sum({file})
             FROM message_media mm JOIN message m ON m._id = mm.message_row_id
             WHERE {in_list} GROUP BY 1, 2"
        );
        if let Ok(mut stmt) = src.prepare(&sql) {
            let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?, r.get::<_, i64>(3)?)));
            if let Ok(rows) = rows {
                for (t, family, n, with_file) in rows.flatten() {
                    if let Some(p) = out.get_mut(&t) {
                        // Only short ASCII words: a MIME family, never anything else.
                        let family = if family.len() <= 16 && family.bytes().all(|b| b.is_ascii_lowercase() || b == b'-') { family } else { "other".to_owned() };
                        *p.media.entry(family).or_default() += n as u64;
                        p.media_with_file += with_file as u64;
                    }
                }
            }
        }
    }

    for (t, p) in &out {
        tracing::warn!(
            message_type = t,
            count = types.get(t).copied().unwrap_or(0),
            tables = ?p.tables,
            columns = ?p.columns,
            media = ?p.media,
            media_with_file = p.media_with_file,
            "unknown message type: where it shows up"
        );
    }
    for table in seen {
        tracing::info!(table = table, columns = ?info.columns(table), "columns of a table that points at unknown messages");
    }
    out
}

fn query_pairs(src: &Connection, sql: &str) -> Vec<(i64, u64)> {
    let run = || -> rusqlite::Result<Vec<(i64, u64)>> {
        let mut stmt = src.prepare(sql)?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)? as u64)))?;
        rows.collect()
    };
    run().unwrap_or_else(|e| {
        tracing::debug!(error = %e, "probe: table query failed");
        Vec::new()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_tables_columns_and_media_of_unknown_types() {
        let src = Connection::open_in_memory().unwrap();
        src.execute_batch(
            "CREATE TABLE chat (_id INTEGER PRIMARY KEY, jid_row_id INTEGER);
             CREATE TABLE jid (_id INTEGER PRIMARY KEY, raw_string TEXT);
             CREATE TABLE message (_id INTEGER PRIMARY KEY, chat_row_id INTEGER, message_type INTEGER, text_data TEXT, from_me INTEGER, timestamp INTEGER);
             CREATE TABLE message_poll (message_row_id INTEGER, selectable_options_count INTEGER);
             CREATE TABLE message_add_on (_id INTEGER PRIMARY KEY, parent_message_row_id INTEGER);
             CREATE TABLE message_media (message_row_id INTEGER, mime_type TEXT, file_path TEXT);
             INSERT INTO message VALUES (1, 1, 66, 'q', 1, 10), (2, 1, 66, NULL, 0, 11), (3, 1, 82, NULL, 0, 12), (4, 1, 0, 'x', 0, 13);
             INSERT INTO message_poll VALUES (1, 1), (2, 1), (4, 1);
             INSERT INTO message_add_on VALUES (1, 1), (2, 1);
             INSERT INTO message_media VALUES (3, 'audio/ogg; codecs=opus', 'Media/x.opus'), (4, 'image/jpeg', NULL);",
        )
        .unwrap();
        let info = SourceInfo::load(&src).unwrap();
        let types = BTreeMap::from([(66, 2), (82, 1)]);
        let out = unknown_types(&src, &info, &types);

        let poll = &out[&66];
        assert_eq!(poll.tables, BTreeMap::from([("message_poll".to_owned(), 2), ("message_add_on^".to_owned(), 1)]));
        assert_eq!(poll.columns, BTreeMap::from([("text_data".to_owned(), 1), ("from_me".to_owned(), 1)]));
        assert!(poll.media.is_empty());

        let voice = &out[&82];
        assert_eq!(voice.tables, BTreeMap::from([("message_media".to_owned(), 1)]));
        assert_eq!(voice.media, BTreeMap::from([("audio".to_owned(), 1)]));
        assert_eq!(voice.media_with_file, 1);
    }
}
