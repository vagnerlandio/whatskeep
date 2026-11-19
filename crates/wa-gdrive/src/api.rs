//! API `backup.googleapis.com/v1` usada pelo app do WhatsApp.
//!
//! * `clients/wa/backups` lista os backups da conta (um por número de telefone).
//! * `<backup>/files` lista os arquivos, com `md5Hash` (base64) e `sizeBytes`.
//! * `<arquivo>?alt=media` baixa o conteúdo.
//!
//! Em backups com criptografia de ponta a ponta, as mídias têm nome ofuscado
//! (`<hash>.mcrypt1`) e o nome real fica em `metadata`, cifrado. O banco de
//! mensagens continua em `Databases/msgstore.db.crypt15`.

use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};

use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use md5::{Digest, Md5};
use reqwest::StatusCode;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};

use crate::auth::AccessToken;
use crate::{Endpoints, Error, http};

/// Limite de tamanho de um arquivo baixado para a memória.
pub const MAX_DOWNLOAD: u64 = 8 << 30;

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Backup {
    /// `clients/wa/backups/<id>`. Não é exibido: o id costuma ser o telefone.
    pub name: String,
    /// Telefone do backup (o id), quando é só dígitos.
    pub phone: Option<String>,
    pub update_time: Option<String>,
    pub size_bytes: Option<u64>,
    /// Ainda sendo enviado pelo celular (incompleto).
    pub uploading: bool,
    pub encrypted: Option<bool>,
    pub chat_db_size: Option<u64>,
    pub media_size: Option<u64>,
    pub messages: Option<u64>,
    pub app_version: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupFile {
    /// Caminho completo na API.
    pub name: String,
    /// Caminho dentro do backup (`Databases/msgstore.db.crypt15`).
    pub path: String,
    pub size_bytes: u64,
    /// MD5 do conteúdo, quando informado.
    #[serde(skip)]
    pub md5: Option<[u8; 16]>,
    /// Metadados cifrados (base64) de arquivos `.mcrypt1`, com o nome real.
    #[serde(skip)]
    pub metadata: Option<String>,
}

#[derive(Deserialize)]
struct RawBackup {
    name: String,
    #[serde(rename = "updateTime")]
    update_time: Option<String>,
    #[serde(rename = "sizeBytes")]
    size_bytes: Option<String>,
    #[serde(rename = "activeTransactionId")]
    active_transaction_id: Option<String>,
    metadata: Option<String>,
}

#[derive(Deserialize)]
struct RawFile {
    name: String,
    #[serde(rename = "sizeBytes")]
    size_bytes: Option<String>,
    #[serde(rename = "md5Hash")]
    md5_hash: Option<String>,
    metadata: Option<String>,
}

pub struct Api<'a> {
    ep: &'a Endpoints,
    token: &'a AccessToken,
    client: Client,
}

impl<'a> Api<'a> {
    pub fn new(ep: &'a Endpoints, token: &'a AccessToken) -> Result<Self, Error> {
        // Sem timeout total: um download grande pode levar muito tempo. O
        // timeout de conexão fica no cliente.
        Ok(Self { ep, token, client: http::client(None)? })
    }

    fn get(&self, path: &str, query: &[(&str, &str)]) -> Result<reqwest::blocking::Response, Error> {
        // Cada segmento do caminho é codificado separadamente (nomes de
        // arquivo vêm do Google e podem ter espaços ou `%`).
        let mut url = url::Url::parse(&self.ep.backup).map_err(|_| Error::Unexpected("URL base inválida"))?;
        url.path_segments_mut()
            .map_err(|_| Error::Unexpected("URL base inválida"))?
            .pop_if_empty()
            .extend(path.split('/').filter(|s| !s.is_empty() && *s != "." && *s != ".."));
        url.query_pairs_mut().extend_pairs(query);
        let resp = self.client.get(url).bearer_auth(self.token.expose()).send()?;
        match resp.status() {
            StatusCode::OK => Ok(resp),
            StatusCode::UNAUTHORIZED => Err(Error::TokenExpired),
            StatusCode::FORBIDDEN => Err(Error::Forbidden),
            StatusCode::NOT_FOUND => Err(Error::NotFound),
            s => Err(Error::Http(s.as_u16())),
        }
    }

    /// Todas as páginas de uma listagem (`<path>` → campo com o último segmento).
    fn list<T: for<'de> Deserialize<'de>>(&self, path: &str) -> Result<Vec<T>, Error> {
        let field = path.rsplit('/').next().unwrap_or(path);
        let mut out = Vec::new();
        let mut page_token: Option<String> = None;
        for _ in 0..10_000 {
            let mut query = vec![("pageSize", "1000")];
            if let Some(t) = page_token.as_deref() {
                query.push(("pageToken", t));
            }
            let page: serde_json::Value = serde_json::from_slice(&self.get(path, &query)?.bytes()?)
                .map_err(|_| Error::Unexpected("JSON inválido na listagem"))?;
            if let Some(items) = page.get(field) {
                out.extend(
                    serde_json::from_value::<Vec<T>>(items.clone())
                        .map_err(|_| Error::Unexpected("formato inesperado na listagem"))?,
                );
            }
            match page.get("nextPageToken").and_then(|v| v.as_str()) {
                Some(t) if !t.is_empty() => page_token = Some(t.to_owned()),
                _ => return Ok(out),
            }
        }
        Err(Error::Unexpected("paginação sem fim"))
    }

    pub fn backups(&self) -> Result<Vec<Backup>, Error> {
        Ok(self.list::<RawBackup>("clients/wa/backups")?.into_iter().map(parse_backup).collect())
    }

    pub fn files(&self, backup_name: &str) -> Result<Vec<BackupFile>, Error> {
        if !is_backup_name(backup_name) {
            return Err(Error::Unexpected("nome de backup inválido"));
        }
        let prefix = format!("{backup_name}/files/");
        Ok(self
            .list::<RawFile>(&format!("{backup_name}/files"))?
            .into_iter()
            .filter_map(|f| {
                let path = f.name.strip_prefix(&prefix)?.to_owned();
                Some(BackupFile {
                    md5: f.md5_hash.as_deref().and_then(|h| B64.decode(h).ok()).and_then(|v| v.try_into().ok()),
                    size_bytes: f.size_bytes.and_then(|s| s.parse().ok()).unwrap_or(0),
                    name: f.name,
                    path,
                    metadata: f.metadata,
                })
            })
            .collect())
    }

    /// Baixa um arquivo para a memória, conferindo tamanho e MD5.
    ///
    /// `progress(baixado, total)` é chamado a cada ~1 MiB. `cancel` interrompe.
    pub fn download(
        &self,
        file: &BackupFile,
        cancel: &AtomicBool,
        mut progress: impl FnMut(u64, u64),
    ) -> Result<Vec<u8>, Error> {
        if file.size_bytes > MAX_DOWNLOAD {
            return Err(Error::TooLarge(file.size_bytes));
        }
        let mut resp = self.get(&file.name, &[("alt", "media")])?;
        let mut out = Vec::with_capacity(file.size_bytes as usize);
        let mut md5 = Md5::new();
        let mut buf = vec![0u8; 256 * 1024];
        let mut last_report = 0u64;
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err(Error::Cancelled);
            }
            let n = resp.read(&mut buf).map_err(|e| Error::Network(e.to_string()))?;
            if n == 0 {
                break;
            }
            if (out.len() + n) as u64 > file.size_bytes.max(1).saturating_add(1 << 20).min(MAX_DOWNLOAD) {
                return Err(Error::Unexpected("o arquivo veio maior que o anunciado"));
            }
            md5.update(&buf[..n]);
            out.extend_from_slice(&buf[..n]);
            if out.len() as u64 - last_report >= 1 << 20 {
                last_report = out.len() as u64;
                progress(last_report, file.size_bytes);
            }
        }
        progress(out.len() as u64, file.size_bytes);
        if file.size_bytes > 0 && out.len() as u64 != file.size_bytes {
            return Err(Error::Incomplete);
        }
        if let Some(expected) = file.md5
            && md5.finalize().as_slice() != expected {
                return Err(Error::Checksum);
            }
        Ok(out)
    }
}

/// `clients/wa/backups/<id>` com id seguro para entrar numa URL.
fn is_backup_name(name: &str) -> bool {
    name.strip_prefix("clients/wa/backups/")
        .is_some_and(|id| !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'))
}

fn parse_backup(raw: RawBackup) -> Backup {
    let meta: serde_json::Value = raw
        .metadata
        .as_deref()
        .and_then(|m| serde_json::from_str(m).ok())
        .unwrap_or_default();
    let num = |k: &str| -> Option<u64> {
        let v = meta.get(k)?;
        v.as_u64().or_else(|| v.as_str()?.parse().ok())
    };
    let id = raw.name.rsplit('/').next().unwrap_or_default();
    Backup {
        phone: (!id.is_empty() && id.chars().all(|c| c.is_ascii_digit())).then(|| id.to_owned()),
        update_time: raw.update_time,
        size_bytes: raw.size_bytes.and_then(|s| s.parse().ok()),
        uploading: raw.active_transaction_id.is_some(),
        encrypted: meta.get("encryptedBackupEnabled").and_then(|v| v.as_bool()),
        chat_db_size: num("chatdbSize"),
        media_size: num("mediaSize"),
        messages: num("numOfMessages"),
        app_version: meta.get("versionOfAppWhenBackup").and_then(|v| v.as_str()).map(str::to_owned),
        name: raw.name,
    }
}

/// O banco de mensagens principal do backup: `Databases/msgstore.db.crypt15`
/// (ou `.crypt14`/`.crypt12`, em backups sem criptografia de ponta a ponta).
pub fn find_msgstore(files: &[BackupFile]) -> Option<&BackupFile> {
    ["crypt15", "crypt14", "crypt12"].iter().find_map(|ext| {
        let want = format!("Databases/msgstore.db.{ext}");
        files.iter().find(|f| f.path == want)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nome_de_backup_seguro() {
        assert!(is_backup_name("clients/wa/backups/5511999999999"));
        assert!(!is_backup_name("clients/wa/backups/../../x"));
        assert!(!is_backup_name("clients/wa/backups/"));
        assert!(!is_backup_name("https://evil/clients/wa/backups/1"));
    }

    #[test]
    fn metadados_do_backup() {
        let b = parse_backup(RawBackup {
            name: "clients/wa/backups/5511999999999".into(),
            update_time: Some("2026-11-01T10:00:00.000Z".into()),
            size_bytes: Some("123456".into()),
            active_transaction_id: None,
            metadata: Some(r#"{"encryptedBackupEnabled":true,"chatdbSize":"5000","numOfMessages":42,"versionOfAppWhenBackup":"2.26.1"}"#.into()),
        });
        assert_eq!(b.phone.as_deref(), Some("5511999999999"));
        assert_eq!((b.size_bytes, b.chat_db_size, b.messages), (Some(123456), Some(5000), Some(42)));
        assert_eq!(b.encrypted, Some(true));
        assert!(!b.uploading);
    }

    #[test]
    fn escolhe_o_msgstore() {
        let f = |p: &str| BackupFile { name: format!("x/files/{p}"), path: p.into(), size_bytes: 1, md5: None, metadata: None };
        let files = [f("Databases/msgstore-2026-01-01.1.db.crypt15"), f("Databases/msgstore.db.crypt14"), f("Databases/msgstore.db.crypt15"), f("Media/ab.mcrypt1")];
        assert_eq!(find_msgstore(&files).unwrap().path, "Databases/msgstore.db.crypt15");
        assert_eq!(find_msgstore(&files[3..]), None);
    }
}
