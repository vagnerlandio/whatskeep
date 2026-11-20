//! Structured content of the kinds that need more than text, stored as JSON in
//! the `card` table and passed to the viewer as is (`Message.card`).
//!
//! Mirrors `Card` in `packages/viewer/src/types.ts`.

use std::collections::{BTreeMap, HashMap};

use rusqlite::{Connection, Row};
use serde::Serialize;

use crate::source::SourceInfo;
use crate::store::Writer;
use crate::Error;

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Card {
    /// Poll: the question is the message text.
    Poll { options: Vec<PollOption>, multiple: bool },
    Event {
        name: Option<String>,
        description: Option<String>,
        start_ms: Option<i64>,
        end_ms: Option<i64>,
        place: Option<String>,
        canceled: bool,
        call: bool,
    },
    Product {
        title: Option<String>,
        description: Option<String>,
        /// Price × 1000, in `currency` (WhatsApp's own unit).
        amount_1000: Option<i64>,
        currency: Option<String>,
    },
    /// Invitation to a group: the text is the invitation's caption.
    Invite { group: Option<String> },
    /// Business message with buttons or a list (template or interactive).
    Buttons { footer: Option<String>, buttons: Vec<String> },
    /// Notice: disappearing messages turned on (`seconds` > 0) or off.
    Disappearing { seconds: i64 },
    /// Notice: advanced chat privacy turned on or off.
    ChatPrivacy { enabled: bool },
    /// Notice: recent messages shared with new group members.
    SharedHistory { messages: Option<i64> },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PollOption {
    pub name: String,
    pub votes: i64,
}

/// Text from WhatsApp's business tables: trimmed, empty → none, and never a
/// JSON blob (some element fields hold one).
pub(crate) fn clean(text: Option<String>) -> Option<String> {
    let t = text?.trim().to_owned();
    (!t.is_empty() && !t.starts_with('{') && !t.starts_with('[')).then_some(t)
}

/// Seconds or milliseconds since 1970 → milliseconds.
pub(crate) fn to_ms(t: Option<i64>) -> Option<i64> {
    t.filter(|&t| t > 0).map(|t| if t < 100_000_000_000 { t * 1000 } else { t })
}

/// `message_type`s whose content comes from other tables.
pub(crate) const CARD_TYPES: &[i64] = &[23, 24, 25, 26, 27, 36, 45, 54, 55, 57, 62, 66, 92, 112, 118];
const TEMPLATES: &[i64] = &[25, 26, 27];
const INTERACTIVE: &[i64] = &[45, 54, 55, 57, 62];

/// Source message id → (app message id, `message_type`), only for `CARD_TYPES`.
pub(crate) type Targets = HashMap<i64, (i64, i64)>;

/// Reads each card's table and writes the cards (and the text some of them
/// keep elsewhere). Returns how many cards of each kind were written.
pub(crate) fn import(src: &Connection, info: &SourceInfo, w: &mut Writer, targets: &Targets) -> Result<BTreeMap<&'static str, u64>, Error> {
    let mut written: BTreeMap<&'static str, u64> = BTreeMap::new();
    if targets.is_empty() {
        return Ok(written);
    }
    let target = |row: i64, types: &[i64]| targets.get(&row).filter(|(_, t)| types.contains(t)).map(|(id, _)| *id);
    let mut put = |w: &mut Writer, id: i64, card: Card, label: &'static str| -> Result<(), Error> {
        w.card(id, &card)?;
        *written.entry(label).or_default() += 1;
        Ok(())
    };

    // Polls: options in order, with their vote count.
    if info.has_col("message_poll_option", "option_name") {
        let multiple: HashMap<i64, i64> = query(
            src,
            &format!("SELECT message_row_id, {} FROM message_poll p", info.col("message_poll", "p", "selectable_options_count")),
            |r| Ok((r.get(0)?, r.get::<_, Option<i64>>(1)?.unwrap_or(1))),
            info.has_col("message_poll", "message_row_id"),
        )?
        .into_iter()
        .collect();
        let mut polls: BTreeMap<i64, Vec<PollOption>> = BTreeMap::new();
        let sql = format!(
            "SELECT message_row_id, option_name, {} FROM message_poll_option o ORDER BY message_row_id, _id",
            info.col("message_poll_option", "o", "vote_total")
        );
        for (row, name, votes) in query(src, &sql, |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, Option<i64>>(2)?)), true)? {
            if let (Some(id), Some(name)) = (target(row, &[66]), name.filter(|n| !n.trim().is_empty())) {
                polls.entry(id).or_default().push(PollOption { name, votes: votes.unwrap_or(0).max(0) });
            }
        }
        for (row, (id, t)) in targets {
            if *t == 66 && let Some(options) = polls.remove(id) {
                put(w, *id, Card::Poll { options, multiple: multiple.get(row).copied().unwrap_or(1) != 1 }, "poll")?;
            }
        }
    }

    if info.has_col("message_event", "message_row_id") {
        let c = |col| info.col("message_event", "e", col);
        let sql = format!(
            "SELECT e.message_row_id, {}, {}, {}, {}, {}, {}, {}, {} FROM message_event e",
            c("name"), c("description"), c("start_time"), c("end_time"), c("location_name"), c("location_address"), c("is_canceled"), c("is_schedule_call")
        );
        for r in query(src, &sql, |r| {
            Ok((
                r.get::<_, i64>(0)?,
                Card::Event {
                    name: clean(r.get(1)?),
                    description: clean(r.get(2)?),
                    start_ms: to_ms(r.get(3)?),
                    end_ms: to_ms(r.get(4)?),
                    place: clean(r.get(5)?).or(clean(r.get(6)?)),
                    canceled: r.get::<_, Option<i64>>(7)?.unwrap_or(0) != 0,
                    call: r.get::<_, Option<i64>>(8)?.unwrap_or(0) != 0,
                },
            ))
        }, true)? {
            if let Some(id) = target(r.0, &[92]) {
                put(w, id, r.1, "event")?;
            }
        }
    }

    if info.has_col("message_product", "message_row_id") {
        let c = |col| info.col("message_product", "p", col);
        let sql = format!(
            "SELECT p.message_row_id, {}, {}, {}, {}, {} FROM message_product p",
            c("title"), c("description"), c("amount_1000"), c("currency_code"), c("body")
        );
        for (row, card, body) in query(src, &sql, |r| {
            Ok((
                r.get::<_, i64>(0)?,
                Card::Product {
                    title: clean(r.get(1)?),
                    description: clean(r.get(2)?),
                    amount_1000: r.get::<_, Option<i64>>(3)?.filter(|a| *a >= 0),
                    currency: clean(r.get::<_, Option<String>>(4)?).filter(|c| c.len() == 3 && c.bytes().all(|b| b.is_ascii_uppercase())),
                },
                clean(r.get(5)?),
            ))
        }, true)? {
            if let Some(id) = target(row, &[23]) {
                if let Some(body) = body {
                    w.fill_text(id, &body)?;
                }
                put(w, id, card, "product")?;
            }
        }
    }

    if info.has_col("message_group_invite", "message_row_id") {
        let sql = format!("SELECT i.message_row_id, {} FROM message_group_invite i", info.col("message_group_invite", "i", "group_name"));
        for (row, group) in query(src, &sql, |r| Ok((r.get::<_, i64>(0)?, clean(r.get(1)?))), true)? {
            if let Some(id) = target(row, &[24]) {
                put(w, id, Card::Invite { group }, "invite")?;
            }
        }
    }

    // Templates: body and footer in `message_template`, buttons in `message_template_button`.
    let mut buttons: HashMap<i64, Vec<String>> = HashMap::new();
    if info.has_col("message_template_button", "text_data") {
        for (row, text) in query(
            src,
            "SELECT message_row_id, text_data FROM message_template_button ORDER BY message_row_id, _id",
            |r| Ok((r.get::<_, i64>(0)?, clean(r.get(1)?))),
            info.has_col("message_template_button", "message_row_id"),
        )? {
            if let (Some(id), Some(text)) = (target(row, TEMPLATES), text) {
                buttons.entry(id).or_default().push(text);
            }
        }
    }
    if info.has_col("message_template", "message_row_id") {
        let sql = format!(
            "SELECT t.message_row_id, {}, {} FROM message_template t",
            info.col("message_template", "t", "content_text_data"),
            info.col("message_template", "t", "footer_text_data")
        );
        for (row, body, footer) in query(src, &sql, |r| Ok((r.get::<_, i64>(0)?, clean(r.get(1)?), clean(r.get(2)?))), true)? {
            let Some(id) = target(row, TEMPLATES) else { continue };
            if let Some(body) = body {
                w.fill_text(id, &body)?;
            }
            let buttons = buttons.remove(&id).unwrap_or_default();
            if footer.is_some() || !buttons.is_empty() {
                put(w, id, Card::Buttons { footer, buttons }, "buttons")?;
            }
        }
    }

    // Interactive business messages: one or more elements per message.
    if info.has_col("message_ui_elements", "message_row_id") {
        let c = |col| info.col("message_ui_elements", "u", col);
        let sql = format!(
            "SELECT u.message_row_id, {}, {}, {}, {} FROM message_ui_elements u ORDER BY u.message_row_id, u._id",
            c("description"), c("element_content"), c("button_text"), c("footer_text")
        );
        #[derive(Default)]
        struct Found {
            body: Option<String>,
            footer: Option<String>,
            buttons: Vec<String>,
        }
        let mut found: BTreeMap<i64, Found> = BTreeMap::new();
        let rows = query(src, &sql, |r| Ok((r.get::<_, i64>(0)?, clean(r.get(1)?), clean(r.get(2)?), clean(r.get(3)?), clean(r.get(4)?))), info.has_col("message_ui_elements", "_id"))?;
        for (row, description, content, button, footer) in rows {
            let Some(id) = target(row, INTERACTIVE) else { continue };
            let e = found.entry(id).or_default();
            if e.body.is_none() {
                e.body = description.or(content);
            }
            if e.footer.is_none() {
                e.footer = footer;
            }
            if let Some(b) = button.filter(|b| !e.buttons.contains(b)) {
                e.buttons.push(b);
            }
        }
        for (id, Found { body, footer, buttons }) in found {
            if let Some(body) = body {
                w.fill_text(id, &body)?;
            }
            if footer.is_some() || !buttons.is_empty() {
                put(w, id, Card::Buttons { footer, buttons }, "buttons")?;
            }
        }
    }

    // Notices: the last row of each setting wins.
    struct Notice {
        table: &'static str,
        col: &'static str,
        code: i64,
        make: fn(Option<i64>) -> Card,
        label: &'static str,
    }
    let notices = [
        Notice {
            table: "message_ephemeral_setting",
            col: "setting_duration",
            code: 36,
            make: |v| Card::Disappearing { seconds: v.unwrap_or(0).max(0) },
            label: "disappearing",
        },
        Notice {
            table: "message_limit_sharing_setting",
            col: "enabled",
            code: 112,
            make: |v| Card::ChatPrivacy { enabled: v.unwrap_or(0) != 0 },
            label: "chatPrivacy",
        },
        Notice {
            table: "group_history_metadata",
            col: "message_count",
            code: 118,
            make: |v| Card::SharedHistory { messages: v.filter(|n| *n > 0) },
            label: "sharedHistory",
        },
    ];
    for Notice { table, col, code, make, label } in notices {
        if !info.has_col(table, "message_row_id") || !info.has_col(table, col) {
            continue;
        }
        let sql = format!("SELECT message_row_id, \"{col}\" FROM \"{table}\" ORDER BY rowid");
        let mut last: BTreeMap<i64, Option<i64>> = BTreeMap::new();
        for (row, v) in query(src, &sql, |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Option<i64>>(1)?)), true)? {
            if let Some(id) = target(row, &[code]) {
                last.insert(id, v);
            }
        }
        for (id, v) in last {
            put(w, id, make(v), label)?;
        }
    }
    Ok(written)
}

/// Runs `sql` when `enabled` (the columns it needs exist).
fn query<T>(src: &Connection, sql: &str, f: impl FnMut(&Row) -> rusqlite::Result<T>, enabled: bool) -> Result<Vec<T>, Error> {
    if !enabled {
        return Ok(Vec::new());
    }
    let mut stmt = src.prepare(sql)?;
    let rows = stmt.query_map([], f)?.collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_shape_matches_the_viewer() {
        let poll = Card::Poll { options: vec![PollOption { name: "A".into(), votes: 2 }], multiple: false };
        assert_eq!(serde_json::to_string(&poll).unwrap(), r#"{"type":"poll","options":[{"name":"A","votes":2}],"multiple":false}"#);
        let ev = Card::Event { name: None, description: None, start_ms: Some(1), end_ms: None, place: None, canceled: false, call: true };
        assert!(serde_json::to_string(&ev).unwrap().contains(r#""type":"event","name":null,"description":null,"startMs":1"#));
        let p = Card::Product { title: None, description: None, amount_1000: Some(5), currency: None };
        assert!(serde_json::to_string(&p).unwrap().contains(r#""amount1000":5"#));
        assert_eq!(serde_json::to_string(&Card::ChatPrivacy { enabled: true }).unwrap(), r#"{"type":"chatPrivacy","enabled":true}"#);
    }

    #[test]
    fn cleans_business_text_and_times() {
        assert_eq!(clean(Some("  oi ".into())), Some("oi".into()));
        assert_eq!(clean(Some("{\"a\":1}".into())), None);
        assert_eq!(clean(Some("   ".into())), None);
        assert_eq!(to_ms(Some(1_700_000_000)), Some(1_700_000_000_000));
        assert_eq!(to_ms(Some(1_700_000_000_000)), Some(1_700_000_000_000));
        assert_eq!(to_ms(Some(0)), None);
    }
}
