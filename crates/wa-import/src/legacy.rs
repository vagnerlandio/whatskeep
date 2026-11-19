//! Schema legado do msgstore: `messages` com `key_remote_jid` em texto.
//!
//! Limitação conhecida: nos bancos mais antigos, o caminho do arquivo de mídia
//! fica num objeto Java serializado (`thumb_image`), que não é lido. Nesses
//! casos a mídia é importada sem `file_path`, com nome, tipo e miniatura.

use std::collections::HashMap;

use rusqlite::Connection;

use crate::jid::{chat_kind, ChatKind};
use crate::kind::{classify, Classify};
use crate::modern::int;
use crate::source::SourceInfo;
use crate::store::{NewMedia, NewMessage, Writer};
use crate::{Error, ImportReport, MessageKind};

const JPEG_MAGIC: &[u8] = &[0xFF, 0xD8, 0xFF];

pub(crate) fn import(src: &Connection, info: &SourceInfo, dst: &Connection, report: &mut ImportReport) -> Result<(), Error> {
    let mut w = Writer::new(dst);

    // --- Conversas ---------------------------------------------------------
    let mut chats: HashMap<String, (i64, ChatKind)> = HashMap::new();
    let chat_sql = if info.has_col("chat_list", "key_remote_jid") {
        format!(
            "SELECT key_remote_jid, {subject}, {archived}, {hidden} FROM chat_list c",
            subject = info.col("chat_list", "c", "subject"),
            archived = info.col("chat_list", "c", "archived"),
            hidden = info.col("chat_list", "c", "hidden"),
        )
    } else {
        "SELECT DISTINCT key_remote_jid, NULL, NULL, NULL FROM messages".to_owned()
    };
    {
        let mut stmt = src.prepare(&chat_sql)?;
        let mut rows = stmt.query([])?;
        while let Some(r) = rows.next()? {
            let Some(jid) = r.get::<_, Option<String>>(0)? else { continue };
            if jid.is_empty() || jid == "-1" || chats.contains_key(&jid) {
                continue;
            }
            let kind = chat_kind(&jid);
            if kind == ChatKind::Status {
                continue;
            }
            let subject: Option<String> = r.get(1)?;
            let id = w.chat(
                &jid,
                kind,
                subject.as_deref(),
                r.get::<_, Option<i64>>(2)?.unwrap_or(0) != 0,
                r.get::<_, Option<i64>>(3)?.unwrap_or(0) != 0,
            )?;
            chats.insert(jid, (id, kind));
            report.chats += 1;
        }
    }

    // --- Mensagens ---------------------------------------------------------
    let c = |col: &str| info.col("messages", "m", col);
    let thumbs_table = info.has_col("message_thumbnails", "key_id") && info.has_col("message_thumbnails", "key_remote_jid");
    let sql = format!(
        "SELECT m._id AS id, m.key_remote_jid AS jid, m.key_from_me AS from_me, m.key_id AS key_id,
                {status} AS status, {data} AS data, {ts} AS ts, {wa_type} AS wa_type, {mime} AS mime,
                {size} AS size, {name} AS file_name, {caption} AS caption, {duration} AS duration,
                {lat} AS lat, {lon} AS lon, {remote} AS remote_resource, {starred} AS starred,
                {edit_version} AS edit_version, {raw_data} AS raw_data,
                {q_key} AS q_key, {q_text} AS q_text, {q_from_me} AS q_from_me, {q_sender} AS q_sender,
                {action} AS action, {mm_path} AS file_path, {mm_w} AS width, {mm_h} AS height,
                {thumb} AS thumb
         FROM messages m
         {j_quote} {j_system} {j_media} {j_thumbs}
         WHERE m.key_remote_jid IS NOT NULL AND m.key_remote_jid <> '-1'
         ORDER BY m._id",
        // `media_wa_type` é TEXT no schema legado.
        status = int(c("status")),
        data = c("data"),
        ts = c("timestamp"),
        wa_type = int(c("media_wa_type")),
        mime = c("media_mime_type"),
        size = c("media_size"),
        name = c("media_name"),
        caption = c("media_caption"),
        duration = c("media_duration"),
        lat = c("latitude"),
        lon = c("longitude"),
        remote = c("remote_resource"),
        starred = c("starred"),
        edit_version = c("edit_version"),
        raw_data = c("raw_data"),
        q_key = guarded(info, "messages_quotes", "_id", "q", "key_id", info.has_col("messages", "quoted_row_id")),
        q_text = guarded(info, "messages_quotes", "_id", "q", "data", info.has_col("messages", "quoted_row_id")),
        q_from_me = guarded(info, "messages_quotes", "_id", "q", "key_from_me", info.has_col("messages", "quoted_row_id")),
        q_sender = guarded(info, "messages_quotes", "_id", "q", "remote_resource", info.has_col("messages", "quoted_row_id")),
        action = info.joined_col("message_system", "message_row_id", "ms", "action_type"),
        mm_path = info.joined_col("message_media", "message_row_id", "mm", "file_path"),
        mm_w = info.joined_col("message_media", "message_row_id", "mm", "width"),
        mm_h = info.joined_col("message_media", "message_row_id", "mm", "height"),
        thumb = if thumbs_table { "mt.thumbnail" } else { "NULL" },
        j_quote = if info.has_col("messages", "quoted_row_id") {
            info.left_join("messages_quotes", "q", "_id", "q._id = m.quoted_row_id")
        } else {
            String::new()
        },
        j_system = info.left_join("message_system", "ms", "message_row_id", "ms.message_row_id = m._id"),
        j_media = info.left_join("message_media", "mm", "message_row_id", "mm.message_row_id = m._id"),
        j_thumbs = if thumbs_table {
            "LEFT JOIN message_thumbnails mt ON mt.key_id = m.key_id AND mt.key_remote_jid = m.key_remote_jid AND mt.key_from_me = m.key_from_me"
        } else {
            ""
        },
    );

    let mut ids: HashMap<i64, i64> = HashMap::new();
    {
        let mut stmt = src.prepare(&sql)?;
        let mut rows = stmt.query([])?;
        while let Some(r) = rows.next()? {
            let jid: String = r.get("jid")?;
            let Some(&(chat_id, chat_kind_)) = chats.get(&jid) else {
                if jid == "status@broadcast" {
                    report.skipped_status += 1;
                } else {
                    report.skipped_orphans += 1;
                }
                continue;
            };
            let from_me = r.get::<_, Option<i64>>("from_me")?.unwrap_or(0) != 0;
            let sender_id = match r.get::<_, Option<String>>("remote_resource")? {
                Some(s) if !from_me && chat_kind_ == ChatKind::Group && !s.is_empty() => Some(w.contact(&s)?),
                _ => None,
            };

            let wa_type: i64 = r.get::<_, Option<i64>>("wa_type")?.unwrap_or(0);
            let mime: Option<String> = r.get("mime")?;
            let file_path: Option<String> = r.get("file_path")?;
            let action: Option<i64> = r.get("action")?;
            let (kind, unknown) = classify(&Classify {
                wa_type,
                status: r.get("status")?,
                edit_version: r.get("edit_version")?,
                from_me,
                mime: mime.as_deref(),
                file_path: file_path.as_deref(),
                has_system_action: action.is_some(),
            });
            if unknown {
                *report.unknown_types.entry(wa_type).or_default() += 1;
            }

            let data: Option<String> = r.get("data")?;
            let caption: Option<String> = r.get("caption")?;
            let file_name: Option<String> = r.get("file_name")?;
            // No legado, contatos compartilhados trazem o vCard em `data`
            // e o nome em `media_name`.
            let vcard = (kind == MessageKind::Contact)
                .then(|| data.as_deref().filter(|d| d.starts_with("BEGIN:VCARD")))
                .flatten();
            let text = match kind {
                MessageKind::Deleted | MessageKind::System => None,
                MessageKind::Contact => file_name.as_deref(),
                k if k.has_media() => caption.as_deref(),
                _ => data.as_deref(),
            };

            let key_id: Option<String> = r.get("key_id")?;
            let q_key: Option<String> = r.get("q_key")?;
            let q_text: Option<String> = r.get("q_text")?;
            let q_sender = match r.get::<_, Option<String>>("q_sender")? {
                Some(s) if !s.is_empty() => Some(w.contact(&s)?),
                _ => None,
            };
            let id = w.message(&NewMessage {
                chat_id,
                key_id: key_id.as_deref(),
                from_me,
                sender_id,
                sent_at: r.get::<_, Option<i64>>("ts")?.unwrap_or(0),
                kind,
                wa_type,
                text,
                starred: r.get::<_, Option<i64>>("starred")?.unwrap_or(0) != 0,
                edited: false,
                system_action: action,
                quoted_key_id: q_key.as_deref(),
                quoted_text: q_text.as_deref(),
                quoted_from_me: r.get::<_, Option<i64>>("q_from_me")?.map(|v| v != 0),
                quoted_sender_id: q_sender,
            })?;
            ids.insert(r.get("id")?, id);
            report.messages += 1;

            if let Some(card) = vcard {
                w.vcard(id, card)?;
                report.vcards += 1;
            }
            if kind.has_media() {
                w.media(
                    id,
                    &NewMedia {
                        file_path: file_path.as_deref(),
                        mime: mime.as_deref(),
                        size: r.get("size")?,
                        width: r.get("width")?,
                        height: r.get("height")?,
                        duration_s: r.get("duration")?,
                        file_name: file_name.as_deref(),
                        ..Default::default()
                    },
                )?;
                report.media += 1;
            }
            // Bancos antigos guardam a miniatura JPEG direto em `raw_data`.
            let raw: Option<Vec<u8>> = r.get("raw_data")?;
            let thumb = raw
                .filter(|b| b.starts_with(JPEG_MAGIC))
                .or(r.get::<_, Option<Vec<u8>>>("thumb")?);
            if let Some(t) = thumb {
                w.thumbnail(id, &t)?;
            }
            if matches!(kind, MessageKind::Location | MessageKind::LiveLocation) {
                if let (Some(lat), Some(lon)) = (r.get::<_, Option<f64>>("lat")?, r.get::<_, Option<f64>>("lon")?) {
                    w.location(id, lat, lon, None, None)?;
                }
            }
        }
    }

    if info.has_col("messages_vcards", "vcard") && info.has_col("messages_vcards", "message_row_id") {
        let mut stmt = src.prepare("SELECT message_row_id, vcard FROM messages_vcards ORDER BY _id")?;
        let mut rows = stmt.query([])?;
        while let Some(r) = rows.next()? {
            let (Some(&id), Some(card)) = (ids.get(&r.get::<_, i64>(0)?), r.get::<_, Option<String>>(1)?) else {
                continue;
            };
            // Evita duplicar o vCard que já veio de `data`.
            let dup: bool = dst.query_row(
                "SELECT EXISTS(SELECT 1 FROM vcard WHERE message_id = ?1 AND data = ?2)",
                rusqlite::params![id, card],
                |r| r.get(0),
            )?;
            if !dup {
                w.vcard(id, &card)?;
                report.vcards += 1;
            }
        }
    }
    Ok(())
}

/// Coluna de um LEFT JOIN que só existe se a tabela existir e a condição valer.
fn guarded(info: &SourceInfo, table: &str, join_col: &str, alias: &str, col: &str, cond: bool) -> String {
    if cond {
        info.joined_col(table, join_col, alias, col)
    } else {
        "NULL".to_owned()
    }
}
