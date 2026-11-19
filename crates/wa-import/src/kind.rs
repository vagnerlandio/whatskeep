/// Tipo normalizado de mensagem, gravado como texto no schema do app.
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

/// Dados de uma linha da origem relevantes para classificar a mensagem.
pub(crate) struct Classify<'a> {
    /// `message.message_type` (novo) ou `messages.media_wa_type` (legado).
    pub wa_type: i64,
    pub status: Option<i64>,
    /// `message_future.version` (novo) ou `messages.edit_version` (legado).
    pub edit_version: Option<i64>,
    pub from_me: bool,
    pub mime: Option<&'a str>,
    pub file_path: Option<&'a str>,
    /// Mensagem de sistema identificada por `message_system`.
    pub has_system_action: bool,
}

/// Retorna o tipo e, se o código for desconhecido, `true`.
///
/// Valores de `message_type` confirmados pela referência (WhatsApp-Chat-Exporter)
/// e pela comunidade: 0 texto, 1 imagem, 2 áudio, 3 vídeo, 4 contato, 5 localização,
/// 7 sistema, 9 documento, 13 GIF, 14 vários contatos, 15 apagada, 16 localização
/// em tempo real, 20 figurinha, 42/43 imagem/vídeo de visualização única.
/// Outros valores ficam como `Unknown` e são contados no relatório.
pub(crate) fn classify(c: &Classify) -> (MessageKind, bool) {
    // Apagada: tipo 15, ou edit_version 7 com status 5 (enviada) / 0 (recebida).
    let deleted_by_version = c.edit_version == Some(7)
        && match c.status {
            Some(5) => c.from_me,
            Some(0) => !c.from_me,
            _ => false,
        };
    if c.wa_type == 15 || deleted_by_version {
        return (MessageKind::Deleted, false);
    }
    // No legado, status 6 marca mensagens de sistema.
    if c.has_system_action || c.status == Some(6) || c.wa_type == 7 {
        return (MessageKind::System, false);
    }
    let kind = match c.wa_type {
        0 => MessageKind::Text,
        1 | 42 => MessageKind::Image,
        2 => {
            let voice = c.mime.is_some_and(|m| m.starts_with("audio/ogg"))
                || c.file_path.is_some_and(|p| p.contains("Voice Notes"));
            if voice { MessageKind::Voice } else { MessageKind::Audio }
        }
        3 | 43 => MessageKind::Video,
        4 | 14 => MessageKind::Contact,
        5 => MessageKind::Location,
        9 => MessageKind::Document,
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
        }
    }

    #[test]
    fn tipos_basicos() {
        assert_eq!(classify(&base(0)).0, MessageKind::Text);
        assert_eq!(classify(&base(1)).0, MessageKind::Image);
        assert_eq!(classify(&base(20)).0, MessageKind::Sticker);
        assert_eq!(classify(&base(9999)), (MessageKind::Unknown, true));
    }

    #[test]
    fn audio_vs_mensagem_de_voz() {
        let mut c = base(2);
        c.mime = Some("audio/mp4");
        assert_eq!(classify(&c).0, MessageKind::Audio);
        c.mime = Some("audio/ogg; codecs=opus");
        assert_eq!(classify(&c).0, MessageKind::Voice);
    }

    #[test]
    fn apagadas() {
        assert_eq!(classify(&base(15)).0, MessageKind::Deleted);
        let mut c = base(0);
        c.edit_version = Some(7);
        c.status = Some(0);
        assert_eq!(classify(&c).0, MessageKind::Deleted);
        // status 5 só conta como apagada se a mensagem for minha
        c.status = Some(5);
        assert_eq!(classify(&c).0, MessageKind::Text);
        c.from_me = true;
        assert_eq!(classify(&c).0, MessageKind::Deleted);
    }
}
