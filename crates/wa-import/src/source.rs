use std::collections::{HashMap, HashSet};

use rusqlite::{Connection, MAIN_DB};

use crate::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceSchema {
    /// `message`, `chat`, `jid`, `message_media`... (2021+)
    Modern,
    /// `messages`, `chat_list`...
    Legacy,
}

impl SourceSchema {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            SourceSchema::Modern => "modern",
            SourceSchema::Legacy => "legacy",
        }
    }
}

/// Abre um msgstore descriptografado direto da memória, somente leitura,
/// sem tocar no disco.
///
/// Liga o `cipher_memory_security` do SQLCipher: toda memória alocada pelo
/// SQLite (a cópia do banco e o cache de páginas) é zerada antes de ser
/// liberada. A configuração vale para o processo inteiro.
pub fn open_msgstore(bytes: &[u8]) -> Result<Connection, Error> {
    let mut conn = Connection::open_in_memory()?;
    // temp_store = MEMORY: ordenações grandes (a importação ordena por data)
    // não podem virar arquivos temporários com o texto em claro no TMPDIR.
    conn.execute_batch("PRAGMA cipher_memory_security = ON; PRAGMA temp_store = MEMORY;")?;
    conn.deserialize_read_exact(MAIN_DB, bytes, bytes.len(), true)?;
    // Falha cedo se os bytes não forem um SQLite válido.
    conn.query_row("SELECT count(*) FROM sqlite_master", [], |_| Ok(()))
        .map_err(|_| Error::NotMsgstore)?;
    Ok(conn)
}

/// Tabelas e colunas existentes no msgstore, para montar queries tolerantes a versão.
pub(crate) struct SourceInfo {
    pub kind: SourceSchema,
    tables: HashMap<String, HashSet<String>>,
}

impl SourceInfo {
    pub fn load(conn: &Connection) -> Result<Self, Error> {
        let mut tables = HashMap::new();
        let names: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table'")?
            .query_map([], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        for name in names {
            let cols: HashSet<String> = conn
                .prepare(&format!("PRAGMA table_info(\"{}\")", name.replace('"', "\"\"")))?
                .query_map([], |r| r.get::<_, String>(1))?
                .collect::<Result<_, _>>()?;
            tables.insert(name, cols);
        }

        let has = |t: &str, c: &str| tables.get(t).is_some_and(|cols| cols.contains(c));
        let count = |t: &str| -> i64 {
            conn.query_row(&format!("SELECT count(*) FROM {t}"), [], |r| r.get(0))
                .unwrap_or(0)
        };
        // Bancos de transição têm as duas tabelas. Vale a que tem mensagens,
        // e o schema novo em caso de empate.
        let modern = has("message", "chat_row_id") && has("chat", "jid_row_id") && tables.contains_key("jid");
        let legacy = has("messages", "key_remote_jid");
        let kind = match (modern, legacy) {
            (true, true) if count("message") == 0 && count("messages") > 0 => SourceSchema::Legacy,
            (true, _) => SourceSchema::Modern,
            (false, true) => SourceSchema::Legacy,
            (false, false) => {
                let mut names: Vec<&String> = tables.keys().collect();
                names.sort();
                tracing::warn!(tabelas = ?names, "nenhum schema de mensagens reconhecido");
                return Err(Error::NotMsgstore);
            }
        };
        // Diagnóstico: só nomes de tabelas do WhatsApp e contagens.
        let main: Vec<(&str, i64)> = [
            "message", "messages", "chat", "chat_list", "jid", "jid_map", "message_media", "message_quoted",
            "message_add_on", "message_vcard", "message_location", "message_thumbnail", "message_system",
        ]
        .into_iter()
        .filter(|t| tables.contains_key(*t))
        .map(|t| (t, count(t)))
        .collect();
        tracing::info!(schema = kind.as_str(), tabelas = tables.len(), contagens = ?main, "msgstore reconhecido");
        let features = [
            ("miniaturas", has("message_thumbnail", "message_row_id") || has("media_hash_thumbnail", "media_hash") || has("message_thumbnails", "key_id")),
            ("edicoes", has("message_edit_info", "message_row_id") || has("messages", "edit_version")),
            ("citacoes", has("message_quoted", "message_row_id") || has("messages", "quoted_row_id")),
            ("reacoes", has("message_add_on_reaction", "reaction")),
            ("vcards", has("message_vcard", "vcard") || has("messages_vcards", "vcard")),
            ("lid_para_telefone", has("jid_map", "lid_row_id")),
            ("mensagens_de_sistema", tables.contains_key("message_system")),
            ("midias", has("message_media", "message_row_id") || has("messages", "media_wa_type")),
        ];
        tracing::info!(recursos = ?features, "recursos encontrados no msgstore");
        let mut all: Vec<&String> = tables.keys().collect();
        all.sort();
        tracing::debug!(tabelas = ?all, "tabelas do msgstore");
        Ok(Self { kind, tables })
    }

    pub fn has_col(&self, table: &str, col: &str) -> bool {
        self.tables.get(table).is_some_and(|c| c.contains(col))
    }

    /// `alias.col`, se a coluna existir. Senão, `NULL`.
    pub fn col(&self, table: &str, alias: &str, col: &str) -> String {
        if self.has_col(table, col) {
            format!("{alias}.{col}")
        } else {
            tracing::debug!(tabela = table, coluna = col, existe_tabela = self.tables.contains_key(table), "coluna ausente no msgstore (vira NULL)");
            "NULL".to_owned()
        }
    }

    /// `LEFT JOIN table alias ON cond`, se a tabela existir (e tiver `required_col`).
    pub fn left_join(&self, table: &str, alias: &str, required_col: &str, cond: &str) -> String {
        if self.has_col(table, required_col) {
            format!("LEFT JOIN {table} {alias} ON {cond}")
        } else {
            tracing::debug!(tabela = table, coluna = required_col, "tabela/coluna ausente no msgstore (join ignorado)");
            String::new()
        }
    }

    /// Colunas de um LEFT JOIN só podem ser selecionadas se o join existir.
    pub fn joined_col(&self, table: &str, join_col: &str, alias: &str, col: &str) -> String {
        if self.has_col(table, join_col) {
            self.col(table, alias, col)
        } else {
            "NULL".to_owned()
        }
    }
}
