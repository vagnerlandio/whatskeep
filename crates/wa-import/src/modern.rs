//! Schema novo do msgstore (2021+): `message`, `chat`, `jid`, `message_media`...

use std::collections::{HashMap, HashSet};

use rusqlite::Connection;

use crate::jid::{chat_kind, ChatKind, JidTable};
use crate::kind::{classify, Classify};
use crate::source::SourceInfo;
use crate::store::{NewMedia, NewMessage, Writer};
use crate::{Error, ImportReport, MessageKind};

/// Alguns bancos guardam números como texto, e o rusqlite não converte sozinho.
pub(crate) fn int(expr: String) -> String {
    if expr == "NULL" { expr } else { format!("CAST({expr} AS INTEGER)") }
}

struct ChatRef {
    id: i64,
    kind: ChatKind,
}

pub(crate) fn import(src: &Connection, info: &SourceInfo, dst: &Connection, report: &mut ImportReport) -> Result<(), Error> {
    let jids = JidTable::load(src, info)?;
    let mut w = Writer::new(dst);

    // --- Conversas ---------------------------------------------------------
    let mut chats: HashMap<i64, ChatRef> = HashMap::new();
    let mut status_chats: HashSet<i64> = HashSet::new();
    {
        let sql = format!(
            "SELECT c._id AS id, c.jid_row_id AS jid_row_id, {subject} AS subject,
                    {archived} AS archived, {hidden} AS hidden
             FROM chat c",
            subject = info.col("chat", "c", "subject"),
            archived = info.col("chat", "c", "archived"),
            hidden = info.col("chat", "c", "hidden"),
        );
        let mut stmt = src.prepare(&sql)?;
        let mut rows = stmt.query([])?;
        while let Some(r) = rows.next()? {
            let src_id: i64 = r.get("id")?;
            let Some(jid_row) = r.get::<_, Option<i64>>("jid_row_id")? else { continue };
            let Some(jid) = jids.resolve(jid_row) else { continue };
            if jid.is_empty() || jid == "-1" {
                continue;
            }
            let kind = chat_kind(jid);
            if kind == ChatKind::Status {
                status_chats.insert(src_id);
                continue;
            }
            let subject: Option<String> = r.get("subject")?;
            let id = w.chat(
                jid,
                kind,
                subject.as_deref(),
                r.get::<_, Option<i64>>("archived")?.unwrap_or(0) != 0,
                r.get::<_, Option<i64>>("hidden")?.unwrap_or(0) != 0,
            )?;
            chats.insert(src_id, ChatRef { id, kind });
            report.chats += 1;
        }
    }

    // --- Mensagens ---------------------------------------------------------
    let thumb_hash_join = info.has_col("media_hash_thumbnail", "media_hash")
        && info.has_col("message_media", "file_hash")
        && info.has_col("message_media", "message_row_id");
    let edited = if info.has_col("message_edit_info", "message_row_id") {
        "EXISTS(SELECT 1 FROM message_edit_info e WHERE e.message_row_id = m._id)"
    } else {
        "0"
    };
    let thumbnail = match (
        info.has_col("message_thumbnail", "message_row_id"),
        thumb_hash_join,
    ) {
        (true, true) => "COALESCE(mt.thumbnail, mht.thumbnail)".to_owned(),
        (true, false) => info.col("message_thumbnail", "mt", "thumbnail"),
        (false, true) => info.col("media_hash_thumbnail", "mht", "thumbnail"),
        (false, false) => "NULL".to_owned(),
    };
    let sql = format!(
        "SELECT m._id AS id, m.chat_row_id AS chat_row_id, m.from_me AS from_me, m.key_id AS key_id,
                {sender} AS sender_jid_row_id, m.timestamp AS ts, {msg_type} AS wa_type,
                {text} AS text, {status} AS status, {starred} AS starred, {edited} AS edited,
                {edit_version} AS edit_version,
                mm.message_row_id AS mm_id, {mm_path} AS file_path, {mm_mime} AS mime, {mm_size} AS size,
                {mm_w} AS width, {mm_h} AS height, {mm_dur} AS duration, {mm_name} AS file_name,
                {mm_hash} AS file_hash, {mm_tr} AS transcription,
                {thumbnail} AS thumbnail,
                {lat} AS lat, {lon} AS lon, {place} AS place_name, {addr} AS place_address,
                {q_key} AS q_key, {q_text} AS q_text, {q_from_me} AS q_from_me, {q_sender} AS q_sender,
                {action} AS action
         FROM message m
         {j_media} {j_thumb} {j_hash_thumb} {j_loc} {j_quote} {j_system} {j_future}
         ORDER BY m._id",
        sender = info.col("message", "m", "sender_jid_row_id"),
        msg_type = int(info.col("message", "m", "message_type")),
        text = info.col("message", "m", "text_data"),
        status = int(info.col("message", "m", "status")),
        starred = info.col("message", "m", "starred"),
        edit_version = info.joined_col("message_future", "message_row_id", "mf", "version"),
        mm_path = info.joined_col("message_media", "message_row_id", "mm", "file_path"),
        mm_mime = info.joined_col("message_media", "message_row_id", "mm", "mime_type"),
        mm_size = info.joined_col("message_media", "message_row_id", "mm", "file_size"),
        mm_w = info.joined_col("message_media", "message_row_id", "mm", "width"),
        mm_h = info.joined_col("message_media", "message_row_id", "mm", "height"),
        mm_dur = info.joined_col("message_media", "message_row_id", "mm", "media_duration"),
        mm_name = info.joined_col("message_media", "message_row_id", "mm", "media_name"),
        mm_hash = info.joined_col("message_media", "message_row_id", "mm", "file_hash"),
        mm_tr = info.joined_col("message_media", "message_row_id", "mm", "raw_transcription_text"),
        lat = info.joined_col("message_location", "message_row_id", "ml", "latitude"),
        lon = info.joined_col("message_location", "message_row_id", "ml", "longitude"),
        place = info.joined_col("message_location", "message_row_id", "ml", "place_name"),
        addr = info.joined_col("message_location", "message_row_id", "ml", "place_address"),
        q_key = info.joined_col("message_quoted", "message_row_id", "mq", "key_id"),
        q_text = info.joined_col("message_quoted", "message_row_id", "mq", "text_data"),
        q_from_me = info.joined_col("message_quoted", "message_row_id", "mq", "from_me"),
        q_sender = info.joined_col("message_quoted", "message_row_id", "mq", "sender_jid_row_id"),
        action = info.joined_col("message_system", "message_row_id", "ms", "action_type"),
        // `mm.message_row_id` é selecionado sempre, então o join precisa existir.
        j_media = if info.has_col("message_media", "message_row_id") {
            "LEFT JOIN message_media mm ON mm.message_row_id = m._id".to_owned()
        } else {
            "LEFT JOIN (SELECT NULL AS message_row_id) mm ON 0".to_owned()
        },
        j_thumb = info.left_join("message_thumbnail", "mt", "message_row_id", "mt.message_row_id = m._id"),
        j_hash_thumb = if thumb_hash_join {
            "LEFT JOIN media_hash_thumbnail mht ON mht.media_hash = mm.file_hash".to_owned()
        } else {
            String::new()
        },
        j_loc = info.left_join("message_location", "ml", "message_row_id", "ml.message_row_id = m._id"),
        j_quote = info.left_join("message_quoted", "mq", "message_row_id", "mq.message_row_id = m._id"),
        j_system = info.left_join("message_system", "ms", "message_row_id", "ms.message_row_id = m._id"),
        j_future = info.left_join("message_future", "mf", "message_row_id", "mf.message_row_id = m._id"),
    );

    let mut ids: HashMap<i64, i64> = HashMap::new();
    {
        let mut stmt = src.prepare(&sql)?;
        let mut rows = stmt.query([])?;
        while let Some(r) = rows.next()? {
            let chat_row: Option<i64> = r.get("chat_row_id")?;
            let Some(chat) = chat_row.and_then(|c| chats.get(&c)) else {
                if chat_row.is_some_and(|c| status_chats.contains(&c)) {
                    report.skipped_status += 1;
                } else {
                    report.skipped_orphans += 1;
                }
                continue;
            };

            let from_me = r.get::<_, Option<i64>>("from_me")?.unwrap_or(0) != 0;
            let resolve_contact = |w: &mut Writer, row: Option<i64>| -> Result<Option<i64>, Error> {
                match row.filter(|&id| id > 0).and_then(|id| jids.resolve(id)) {
                    Some(jid) => Ok(Some(w.contact(jid)?)),
                    None => Ok(None),
                }
            };
            let sender_id = if !from_me && chat.kind == ChatKind::Group {
                resolve_contact(&mut w, r.get("sender_jid_row_id")?)?
            } else {
                None
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

            let text: Option<String> = r.get("text")?;
            let key_id: Option<String> = r.get("key_id")?;
            let q_key: Option<String> = r.get("q_key")?;
            let q_text: Option<String> = r.get("q_text")?;
            let q_sender = resolve_contact(&mut w, r.get("q_sender")?)?;
            let id = w.message(&NewMessage {
                chat_id: chat.id,
                key_id: key_id.as_deref(),
                from_me,
                sender_id,
                sent_at: r.get::<_, Option<i64>>("ts")?.unwrap_or(0),
                kind,
                wa_type,
                // Mensagens apagadas às vezes mantêm o texto antigo no banco.
                // Seguimos o WhatsApp e não mostramos.
                text: if kind == MessageKind::Deleted { None } else { text.as_deref() },
                starred: r.get::<_, Option<i64>>("starred")?.unwrap_or(0) != 0,
                edited: r.get::<_, Option<i64>>("edited")?.unwrap_or(0) != 0,
                system_action: action,
                quoted_key_id: q_key.as_deref(),
                quoted_text: q_text.as_deref(),
                quoted_from_me: r.get::<_, Option<i64>>("q_from_me")?.map(|v| v != 0),
                quoted_sender_id: q_sender,
            })?;
            ids.insert(r.get("id")?, id);
            report.messages += 1;

            if r.get::<_, Option<i64>>("mm_id")?.is_some() && kind.has_media() {
                let file_name: Option<String> = r.get("file_name")?;
                let file_hash: Option<String> = r.get("file_hash")?;
                let transcription: Option<String> = r.get("transcription")?;
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
                        file_hash: file_hash.as_deref(),
                        transcription: transcription.as_deref(),
                    },
                )?;
                report.media += 1;
            }
            if let Some(thumb) = r.get::<_, Option<Vec<u8>>>("thumbnail")? {
                w.thumbnail(id, &thumb)?;
            }
            if let (Some(lat), Some(lon)) = (r.get::<_, Option<f64>>("lat")?, r.get::<_, Option<f64>>("lon")?) {
                let name: Option<String> = r.get("place_name")?;
                let addr: Option<String> = r.get("place_address")?;
                w.location(id, lat, lon, name.as_deref(), addr.as_deref())?;
            }
        }
    }

    // --- Contatos compartilhados (vCard) ----------------------------------
    if info.has_col("message_vcard", "vcard") {
        let mut stmt = src.prepare("SELECT message_row_id, vcard FROM message_vcard ORDER BY _id")?;
        let mut rows = stmt.query([])?;
        while let Some(r) = rows.next()? {
            let (Some(&id), Some(card)) = (ids.get(&r.get::<_, i64>(0)?), r.get::<_, Option<String>>(1)?) else {
                continue;
            };
            w.vcard(id, &card)?;
            report.vcards += 1;
        }
    }

    // --- Reações -----------------------------------------------------------
    if info.has_col("message_add_on", "parent_message_row_id") && info.has_col("message_add_on_reaction", "reaction") {
        let sql = format!(
            "SELECT a.parent_message_row_id, a.from_me, {sender}, r.reaction, {ts}
             FROM message_add_on a
             JOIN message_add_on_reaction r ON r.message_add_on_row_id = a._id
             ORDER BY a._id",
            sender = info.col("message_add_on", "a", "sender_jid_row_id"),
            ts = info.col("message_add_on_reaction", "r", "sender_timestamp"),
        );
        let mut stmt = src.prepare(&sql)?;
        let mut rows = stmt.query([])?;
        while let Some(r) = rows.next()? {
            let Some(&message_id) = r.get::<_, Option<i64>>(0)?.and_then(|p| ids.get(&p)) else {
                continue;
            };
            // Reação removida fica gravada com emoji vazio.
            let Some(emoji) = r.get::<_, Option<String>>(3)?.filter(|e| !e.is_empty()) else {
                continue;
            };
            let from_me = r.get::<_, Option<i64>>(1)?.unwrap_or(0) != 0;
            let sender_id = match r.get::<_, Option<i64>>(2)?.filter(|&id| id > 0).and_then(|id| jids.resolve(id)) {
                Some(jid) if !from_me => Some(w.contact(jid)?),
                _ => None,
            };
            w.reaction(message_id, from_me, sender_id, &emoji, r.get(4)?)?;
            report.reactions += 1;
        }
    }
    Ok(())
}
