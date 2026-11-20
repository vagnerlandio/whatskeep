//! Imports the WhatsApp (Android) `msgstore.db` into the app's normalized schema.
//!
//! * Supports the **modern** schema (`message`, `chat`, `jid`, `message_media`...)
//!   and the **legacy** one (`messages`, `chat_list`...).
//! * It is version-tolerant: every optional column and table is checked before
//!   it goes into the query. Whatever does not exist becomes `NULL`.
//! * Group participants identified by LID (`@lid`) are converted
//!   to the phone number via `jid_map`, when the table exists.
//! * Does no file I/O. The source comes from [`open_msgstore`] (bytes in
//!   memory) and the destination is any [`rusqlite::Connection`] (the
//!   SQLCipher vault; in tests an in-memory database).
//!
//! Schema reference: the WhatsApp-Chat-Exporter project (KnugiHK).

pub mod contacts;
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

/// Which app produced the backup.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
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

/// Import summary, to show to the user and for diagnostics.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImportReport {
    pub source_schema: Option<SourceSchema>,
    pub chats: u64,
    pub messages: u64,
    pub media: u64,
    pub reactions: u64,
    pub vcards: u64,
    /// WhatsApp status updates (stories). They are not imported.
    pub skipped_status: u64,
    /// Messages whose chat does not exist in the database (orphan rows).
    pub skipped_orphans: u64,
    /// Unknown `message_type` → count. Helps map new types
    /// when testing with real backups.
    pub unknown_types: BTreeMap<i64, u64>,
}

/// Imports everything from `src` (a msgstore) into `dst`, in a single transaction.
///
/// Creates the schema in `dst` if needed. Fails if `dst` already holds a history:
/// incremental merging of backups is left for a later phase.
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
