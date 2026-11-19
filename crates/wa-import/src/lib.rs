//! Importa o `msgstore.db` do WhatsApp (Android) para o schema normalizado do app.
//!
//! * Suporta o schema **novo** (`message`, `chat`, `jid`, `message_media`...)
//!   e o **legado** (`messages`, `chat_list`...).
//! * É tolerante a versões: cada coluna e tabela opcional é verificada antes
//!   de entrar na query. O que não existir vira `NULL`.
//! * Participantes de grupo identificados por LID (`@lid`) são convertidos
//!   para o número de telefone via `jid_map`, quando a tabela existe.
//! * Não faz I/O de arquivo. A origem vem de [`open_msgstore`] (bytes em
//!   memória) e o destino é qualquer [`rusqlite::Connection`] (o cofre
//!   SQLCipher, nos testes um banco em memória).
//!
//! Referência de schema: projeto WhatsApp-Chat-Exporter (KnugiHK).

mod jid;
mod kind;
mod legacy;
mod modern;
mod source;
pub mod store;

use std::collections::BTreeMap;

use rusqlite::Connection;

pub use kind::MessageKind;
pub use source::{open_msgstore, SourceSchema};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("o arquivo não parece ser um msgstore do WhatsApp (tabelas de mensagens não encontradas)")]
    NotMsgstore,
    #[error("o destino já contém um histórico importado")]
    DestinationNotEmpty,
    #[error("erro no banco de dados: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

/// Qual app gerou o backup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceApp {
    WhatsApp,
    Business,
}

impl SourceApp {
    pub const ALL: [SourceApp; 2] = [SourceApp::WhatsApp, SourceApp::Business];

    pub fn as_str(self) -> &'static str {
        match self {
            SourceApp::WhatsApp => "whatsapp",
            SourceApp::Business => "business",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|a| a.as_str() == s)
    }
}

#[derive(Clone, Debug)]
pub struct ImportOptions {
    pub source_app: SourceApp,
}

/// Resumo da importação, para mostrar ao usuário e para diagnóstico.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImportReport {
    pub source_schema: Option<SourceSchema>,
    pub chats: u64,
    pub messages: u64,
    pub media: u64,
    pub reactions: u64,
    pub vcards: u64,
    /// Status do WhatsApp (stories). Não são importados.
    pub skipped_status: u64,
    /// Mensagens cuja conversa não existe no banco (linhas órfãs).
    pub skipped_orphans: u64,
    /// `message_type` desconhecidos → quantidade. Ajuda a mapear tipos novos
    /// ao testar com backups reais.
    pub unknown_types: BTreeMap<i64, u64>,
}

/// Importa tudo de `src` (um msgstore) para `dst`, numa única transação.
///
/// Cria o schema em `dst` se necessário. Falha se `dst` já tiver um histórico:
/// a mesclagem incremental de backups fica para uma fase posterior.
pub fn import(src: &Connection, dst: &mut Connection, opts: &ImportOptions) -> Result<ImportReport, Error> {
    let schema = source::SourceInfo::load(src)?;
    let tx = dst.transaction()?;
    store::create_schema(&tx)?;
    if store::has_data(&tx)? {
        return Err(Error::DestinationNotEmpty);
    }

    let mut report = ImportReport {
        source_schema: Some(schema.kind),
        ..Default::default()
    };
    match schema.kind {
        SourceSchema::Modern => modern::import(src, &schema, &tx, &mut report)?,
        SourceSchema::Legacy => legacy::import(src, &schema, &tx, &mut report)?,
    }

    store::finish(&tx, opts.source_app.as_str(), schema.kind.as_str())?;
    tx.commit()?;
    Ok(report)
}
