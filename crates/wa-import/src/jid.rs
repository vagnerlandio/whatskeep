use std::collections::HashMap;

use rusqlite::Connection;

use crate::source::SourceInfo;
use crate::Error;

/// Tipo de conversa, derivado do servidor do JID.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ChatKind {
    Direct,
    Group,
    Broadcast,
    Channel,
    /// `status@broadcast` (stories). Não é importado.
    Status,
}

impl ChatKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ChatKind::Direct => "direct",
            ChatKind::Group => "group",
            ChatKind::Broadcast => "broadcast",
            ChatKind::Channel => "channel",
            ChatKind::Status => "status",
        }
    }
}

pub(crate) fn chat_kind(jid: &str) -> ChatKind {
    if jid == "status@broadcast" {
        return ChatKind::Status;
    }
    match jid.rsplit_once('@').map(|(_, s)| s) {
        Some("g.us") => ChatKind::Group,
        Some("broadcast") => ChatKind::Broadcast,
        Some("newsletter") => ChatKind::Channel,
        _ => ChatKind::Direct,
    }
}

/// Número de telefone (só dígitos) de um JID de usuário, se houver.
/// LIDs (`@lid`) não são telefones.
pub(crate) fn phone_of(jid: &str) -> Option<&str> {
    let (user, server) = jid.split_once('@')?;
    // JIDs de dispositivo vêm como "5511...:12@s.whatsapp.net"
    let user = user.split(':').next()?;
    (server == "s.whatsapp.net" && !user.is_empty() && user.bytes().all(|b| b.is_ascii_digit())).then_some(user)
}

/// Tabela `jid` do schema novo, com a resolução LID → telefone via `jid_map`.
pub(crate) struct JidTable {
    raw: HashMap<i64, String>,
    lid_to_jid: HashMap<i64, i64>,
}

impl JidTable {
    pub fn load(src: &Connection, info: &SourceInfo) -> Result<Self, Error> {
        let raw_expr = if info.has_col("jid", "raw_string") {
            "COALESCE(raw_string, user || '@' || server)"
        } else {
            "user || '@' || server"
        };
        let raw = src
            .prepare(&format!("SELECT _id, {raw_expr} FROM jid"))?
            .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Option<String>>(1)?)))?
            .filter_map(|r| match r {
                Ok((id, Some(s))) => Some(Ok((id, s))),
                Ok((_, None)) => None,
                Err(e) => Some(Err(e)),
            })
            .collect::<Result<_, _>>()?;

        let lid_to_jid = if info.has_col("jid_map", "lid_row_id") && info.has_col("jid_map", "jid_row_id") {
            src.prepare("SELECT lid_row_id, jid_row_id FROM jid_map")?
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<Result<_, _>>()?
        } else {
            HashMap::new()
        };
        Ok(Self { raw, lid_to_jid })
    }

    /// JID canônico: o telefone, quando o LID tem mapeamento.
    pub fn resolve(&self, row_id: i64) -> Option<&str> {
        let id = self.lid_to_jid.get(&row_id).copied().unwrap_or(row_id);
        self.raw.get(&id).or_else(|| self.raw.get(&row_id)).map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tipos_de_conversa() {
        assert_eq!(chat_kind("5511999999999@s.whatsapp.net"), ChatKind::Direct);
        assert_eq!(chat_kind("123-456@g.us"), ChatKind::Group);
        assert_eq!(chat_kind("status@broadcast"), ChatKind::Status);
        assert_eq!(chat_kind("1234@broadcast"), ChatKind::Broadcast);
        assert_eq!(chat_kind("1234@newsletter"), ChatKind::Channel);
    }

    #[test]
    fn telefone() {
        assert_eq!(phone_of("5511999999999@s.whatsapp.net"), Some("5511999999999"));
        assert_eq!(phone_of("5511999999999:3@s.whatsapp.net"), Some("5511999999999"));
        assert_eq!(phone_of("98765432@lid"), None);
        assert_eq!(phone_of("123-456@g.us"), None);
    }
}
