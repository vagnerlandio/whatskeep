/// Normalized message type, stored as text in the app's schema.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MessageKind {
    Text,
    Image,
    Audio,
    Voice,
    Video,
    Gif,
    Sticker,
    Document,
    Contact,
    Location,
    LiveLocation,
    System,
    Deleted,
    Poll,
    Event,
    Product,
    /// Invitation to join a group.
    Invite,
    /// The phone never got the content ("Waiting for this message").
    Waiting,
    /// Someone mentioned the chat in a status.
    StatusMention,
    /// Container of an album: its photos and videos are messages of their own,
    /// so it is not imported (its reactions move to the first item).
    Album,
    Unknown,
}

impl MessageKind {
    pub fn as_str(self) -> &'static str {
        match self {
            MessageKind::Text => "text",
            MessageKind::Image => "image",
            MessageKind::Audio => "audio",
            MessageKind::Voice => "voice",
            MessageKind::Video => "video",
            MessageKind::Gif => "gif",
            MessageKind::Sticker => "sticker",
            MessageKind::Document => "document",
            MessageKind::Contact => "contact",
            MessageKind::Location => "location",
            MessageKind::LiveLocation => "live_location",
            MessageKind::System => "system",
            MessageKind::Deleted => "deleted",
            MessageKind::Poll => "poll",
            MessageKind::Event => "event",
            MessageKind::Product => "product",
            MessageKind::Invite => "invite",
            MessageKind::Waiting => "waiting",
            MessageKind::StatusMention => "status_mention",
            MessageKind::Album => "album",
            MessageKind::Unknown => "unknown",
        }
    }

    pub fn has_media(self) -> bool {
        matches!(
            self,
            MessageKind::Image
                | MessageKind::Audio
                | MessageKind::Voice
                | MessageKind::Video
                | MessageKind::Gif
                | MessageKind::Sticker
                | MessageKind::Document
        )
    }
}

/// Data from a source row relevant to classifying the message.
pub(crate) struct Classify<'a> {
    /// `message.message_type` (modern) or `messages.media_wa_type` (legacy).
    pub wa_type: i64,
    pub status: Option<i64>,
    /// `message_future.version` (modern) or `messages.edit_version` (legacy).
    pub edit_version: Option<i64>,
    pub from_me: bool,
    pub mime: Option<&'a str>,
    pub file_path: Option<&'a str>,
    /// System message identified by `message_system`.
    pub has_system_action: bool,
    /// Modern schema (`message.message_type`). The codes mapped from a real
    /// backup (`MODERN_ONLY`) were only seen there: the legacy
    /// `media_wa_type` keeps them unknown.
    pub modern: bool,
}

/// Codes mapped from a modern-schema backup.
const MODERN_ONLY: &[i64] = &[11, 23, 24, 25, 26, 27, 32, 36, 45, 46, 49, 54, 55, 57, 62, 64, 66, 81, 82, 92, 99, 103, 112, 118];

/// Returns the type and, if the code is unknown, `true`.
///
/// `message_type` values confirmed by the reference (WhatsApp-Chat-Exporter)
/// and by the community: 0 text, 1 image, 2 audio, 3 video, 4 contact, 5 location,
/// 7 system, 9 document, 13 GIF, 14 multiple contacts, 15 deleted, 16 live
/// location, 20 sticker, 42/43 view-once image/video.
///
/// Mapped from a real backup by the tables that point at each type (see
/// `probe.rs`), not by guessing: 11 waiting for the message (no content),
/// 23 product (`message_product`), 24 group invite (`message_group_invite`),
/// 25/26/27 business template with an image / document / no header
/// (`message_template`), 32/46/49 reply to a button or list (text),
/// 36 disappearing messages setting (`message_ephemeral_setting`),
/// 45/54/55 business message with buttons or a list and 57/62 the same with
/// an image / video (`message_ui_elements`), 64 deleted (`message_revoked`),
/// 66 poll (`message_poll`), 81 round video message, 82 view-once voice
/// message (`message_view_once_media`), 92 event (`message_event`), 99 album
/// (`message_album`), 103 the chat mentioned in a status (quotes the status),
/// 112 advanced chat privacy setting (`message_limit_sharing_setting`),
/// 118 history shared with new group members (`group_history_metadata`).
/// Other values stay `Unknown` and are counted in the report.
pub(crate) fn classify(c: &Classify) -> (MessageKind, bool) {
    let wa_type = if !c.modern && MODERN_ONLY.contains(&c.wa_type) { -1 } else { c.wa_type };
    // Deleted: type 15, or edit_version 7 with status 5 (sent) / 0 (received).
    let deleted_by_version = c.edit_version == Some(7)
        && match c.status {
            Some(5) => c.from_me,
            Some(0) => !c.from_me,
            _ => false,
        };
    if wa_type == 15 || wa_type == 64 || deleted_by_version {
        return (MessageKind::Deleted, false);
    }
    // In the legacy schema, status 6 marks system messages.
    if c.has_system_action || c.status == Some(6) || matches!(wa_type, 7 | 36 | 112 | 118) {
        return (MessageKind::System, false);
    }
    let kind = match wa_type {
        0 | 27 | 32 | 45 | 46 | 49 | 54 | 55 => MessageKind::Text,
        1 | 42 | 25 | 57 => MessageKind::Image,
        2 => {
            let voice = c.mime.is_some_and(|m| m.starts_with("audio/ogg"))
                || c.file_path.is_some_and(|p| p.contains("Voice Notes"));
            if voice { MessageKind::Voice } else { MessageKind::Audio }
        }
        3 | 43 | 62 | 81 => MessageKind::Video,
        4 | 14 => MessageKind::Contact,
        5 => MessageKind::Location,
        9 | 26 => MessageKind::Document,
        11 => MessageKind::Waiting,
        23 => MessageKind::Product,
        24 => MessageKind::Invite,
        66 => MessageKind::Poll,
        82 => MessageKind::Voice,
        92 => MessageKind::Event,
        99 => MessageKind::Album,
        103 => MessageKind::StatusMention,
        13 => MessageKind::Gif,
        16 => MessageKind::LiveLocation,
        20 => MessageKind::Sticker,
        _ => return (MessageKind::Unknown, true),
    };
    (kind, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base(wa_type: i64) -> Classify<'static> {
        Classify {
            wa_type,
            status: Some(13),
            edit_version: None,
            from_me: false,
            mime: None,
            file_path: None,
            has_system_action: false,
            modern: true,
        }
    }

    #[test]
    fn basic_types() {
        assert_eq!(classify(&base(0)).0, MessageKind::Text);
        assert_eq!(classify(&base(1)).0, MessageKind::Image);
        assert_eq!(classify(&base(20)).0, MessageKind::Sticker);
        assert_eq!(classify(&base(9999)), (MessageKind::Unknown, true));
    }

    #[test]
    fn types_mapped_from_a_real_backup() {
        let cases = [
            (11, MessageKind::Waiting),
            (23, MessageKind::Product),
            (24, MessageKind::Invite),
            (25, MessageKind::Image),
            (26, MessageKind::Document),
            (27, MessageKind::Text),
            (32, MessageKind::Text),
            (36, MessageKind::System),
            (45, MessageKind::Text),
            (46, MessageKind::Text),
            (49, MessageKind::Text),
            (54, MessageKind::Text),
            (55, MessageKind::Text),
            (57, MessageKind::Image),
            (62, MessageKind::Video),
            (64, MessageKind::Deleted),
            (66, MessageKind::Poll),
            (81, MessageKind::Video),
            (82, MessageKind::Voice),
            (92, MessageKind::Event),
            (99, MessageKind::Album),
            (103, MessageKind::StatusMention),
            (112, MessageKind::System),
            (118, MessageKind::System),
        ];
        for (code, kind) in cases {
            assert_eq!(classify(&base(code)), (kind, false), "message_type {code}");
        }
        // Still unknown: nothing in the backup says what they are.
        assert!(classify(&base(87)).1);
        assert!(classify(&base(116)).1);
    }

    #[test]
    fn legacy_schema_keeps_the_new_codes_unknown() {
        let mut c = base(66);
        c.modern = false;
        assert_eq!(classify(&c), (MessageKind::Unknown, true));
        // ...but what marks a message as deleted or system still applies.
        c.has_system_action = true;
        assert_eq!(classify(&c).0, MessageKind::System);
    }

    #[test]
    fn audio_vs_voice_message() {
        let mut c = base(2);
        c.mime = Some("audio/mp4");
        assert_eq!(classify(&c).0, MessageKind::Audio);
        c.mime = Some("audio/ogg; codecs=opus");
        assert_eq!(classify(&c).0, MessageKind::Voice);
    }

    #[test]
    fn deleted_messages() {
        assert_eq!(classify(&base(15)).0, MessageKind::Deleted);
        let mut c = base(0);
        c.edit_version = Some(7);
        c.status = Some(0);
        assert_eq!(classify(&c).0, MessageKind::Deleted);
        // status 5 only counts as deleted if the message is mine
        c.status = Some(5);
        assert_eq!(classify(&c).0, MessageKind::Text);
        c.from_me = true;
        assert_eq!(classify(&c).0, MessageKind::Deleted);
    }
}
