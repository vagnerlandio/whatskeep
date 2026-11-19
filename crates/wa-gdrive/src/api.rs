//! The `backup.googleapis.com/v1` API used by the WhatsApp app.
//!
//! * `clients/wa/backups` lists the account's backups (one per phone number).
//! * `<backup>/files` lists the files, with `md5Hash` (base64) and `sizeBytes`.
//! * `<file>?alt=media` downloads the content.
//!
//! In end-to-end encrypted backups, media files have obfuscated names
//! (`<hash>.mcrypt1`) and the real name is in `metadata`, encrypted. The
//! message database is still `Databases/msgstore.db.crypt15`.

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

/// Size limit for a file downloaded into memory.
pub const MAX_DOWNLOAD: u64 = 8 << 30;

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Backup {
    /// `clients/wa/backups/<id>`. Not displayed: the id is usually the phone number.
    pub name: String,
    /// The backup's phone number (the id), when it is digits only.
    pub phone: Option<String>,
    pub update_time: Option<String>,
    pub size_bytes: Option<u64>,
    /// Still being uploaded by the phone (incomplete).
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
    /// Full path in the API.
    pub name: String,
    /// Path inside the backup (`Databases/msgstore.db.crypt15`).
    pub path: String,
    pub size_bytes: u64,
    /// MD5 of the content, when provided.
    #[serde(skip)]
    pub md5: Option<[u8; 16]>,
    /// Encrypted metadata (base64) of `.mcrypt1` files, with the real name.
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
        // No overall timeout: a large download can take a long time. The
        // connect timeout is set on the client.
        Ok(Self { ep, token, client: http::client(None)? })
    }

    fn get(&self, path: &str, query: &[(&str, &str)]) -> Result<reqwest::blocking::Response, Error> {
        // Each path segment is encoded separately (file names come from
        // Google and may contain spaces or `%`).
        let mut url = url::Url::parse(&self.ep.backup).map_err(|_| Error::Unexpected("URL base inválida"))?;
        url.path_segments_mut()
            .map_err(|_| Error::Unexpected("URL base inválida"))?
            .pop_if_empty()
            .extend(path.split('/').filter(|s| !s.is_empty() && *s != "." && *s != ".."));
        url.query_pairs_mut().extend_pairs(query);
        let shown = format!(
            "{}/{}{}",
            url.host_str().unwrap_or(""),
            crate::trace::path(path),
            if query.is_empty() {
                String::new()
            } else {
                // pageToken is opaque, but it does not need to be shown.
                let q: Vec<String> = query
                    .iter()
                    .map(|(k, v)| if *k == "pageToken" { format!("{k}=<redacted>") } else { format!("{k}={v}") })
                    .collect();
                format!("?{}", q.join("&"))
            }
        );
        let started = std::time::Instant::now();
        tracing::debug!(target: "wa_gdrive::http", "→ GET {shown} (Authorization: Bearer <redacted>)");
        let resp = match self.client.get(url).bearer_auth(self.token.expose()).send() {
            Ok(r) => r,
            Err(e) => {
                let err: Error = e.into();
                tracing::warn!(target: "wa_gdrive::http", ms = started.elapsed().as_millis() as u64, "✗ GET {shown} failed: {}", err);
                return Err(err);
            }
        };
        let status = resp.status();
        let ms = started.elapsed().as_millis() as u64;
        let content_type = resp.headers().get("content-type").and_then(|v| v.to_str().ok()).unwrap_or("").to_owned();
        let length = resp.content_length();
        if status == StatusCode::OK {
            tracing::debug!(target: "wa_gdrive::http", status = status.as_u16(), ms, content_type, length, "← GET {shown}");
            return Ok(resp);
        }
        // Error: the body (Google's error JSON) helps understand the reason.
        let body = resp.bytes().map(|b| b.to_vec()).unwrap_or_default();
        let detail = match serde_json::from_slice::<serde_json::Value>(&body) {
            Ok(v) => crate::trace::json(&v).to_string(),
            Err(_) => crate::trace::text_snippet(&body),
        };
        tracing::warn!(target: "wa_gdrive::http", status = status.as_u16(), ms, content_type, "← GET {shown}: {detail}");
        match status {
            StatusCode::UNAUTHORIZED => Err(Error::TokenExpired),
            StatusCode::FORBIDDEN => Err(Error::Forbidden),
            StatusCode::NOT_FOUND => Err(Error::NotFound),
            s => Err(Error::Http(s.as_u16())),
        }
    }

    /// All pages of a listing (`<path>` → field named after the last segment).
    fn list<T: for<'de> Deserialize<'de>>(&self, path: &str) -> Result<Vec<T>, Error> {
        let field = path.rsplit('/').next().unwrap_or(path);
        let mut out = Vec::new();
        let mut page_token: Option<String> = None;
        for _ in 0..10_000 {
            let mut query = vec![("pageSize", "1000")];
            if let Some(t) = page_token.as_deref() {
                query.push(("pageToken", t));
            }
            let body = self.get(path, &query)?.bytes()?;
            let page: serde_json::Value = match serde_json::from_slice(&body) {
                Ok(v) => v,
                Err(_) => {
                    tracing::warn!(target: "wa_gdrive::http", "listing is not JSON: {}", crate::trace::text_snippet(&body));
                    return Err(Error::Unexpected("JSON inválido na listagem"));
                }
            };
            tracing::debug!(target: "wa_gdrive::http", bytes = body.len(), "  body: {}", crate::trace::json(&page));
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

    /// Downloads a file into memory, checking size and MD5.
    ///
    /// `progress(downloaded, total)` is called every ~1 MiB. `cancel` aborts.
    pub fn download(
        &self,
        file: &BackupFile,
        cancel: &AtomicBool,
        mut progress: impl FnMut(u64, u64),
    ) -> Result<Vec<u8>, Error> {
        if file.size_bytes > MAX_DOWNLOAD {
            return Err(Error::TooLarge(file.size_bytes));
        }
        let started = std::time::Instant::now();
        let shown = crate::trace::file_path(&file.path);
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
        let ms = started.elapsed().as_millis() as u64;
        if file.size_bytes > 0 && out.len() as u64 != file.size_bytes {
            tracing::warn!(target: "wa_gdrive::http", received = out.len(), announced = file.size_bytes, ms, "incomplete download: {shown}");
            return Err(Error::Incomplete);
        }
        let md5_ok = file.md5.map(|expected| md5.finalize().as_slice() == expected);
        let mbps = out.len() as f64 / 1_048_576.0 / (ms.max(1) as f64 / 1000.0);
        if md5_ok == Some(false) {
            tracing::warn!(target: "wa_gdrive::http", bytes = out.len(), ms, "MD5 mismatch: {shown}");
            return Err(Error::Checksum);
        }
        tracing::debug!(target: "wa_gdrive::http", bytes = out.len(), ms, md5 = ?md5_ok, "downloaded {shown} ({mbps:.1} MB/s)");
        Ok(out)
    }
}

/// `clients/wa/backups/<id>` with an id that is safe to put in a URL.
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
        // Old format: `encryptedBackupEnabled`. Current (seen in 2026-11):
        // `encryptedBackupMetadata.isPasswordOrEncryptionKeyEncrypted`.
        encrypted: meta.get("encryptedBackupEnabled").and_then(|v| v.as_bool()).or_else(|| {
            let m = meta.get("encryptedBackupMetadata")?;
            m.get("isPasswordOrEncryptionKeyEncrypted").and_then(|v| v.as_bool()).or_else(|| {
                let key = m.get("isEncryptionKeyEncrypted")?.as_bool()?;
                let pw = m.get("isPasswordEncrypted")?.as_bool()?;
                Some(key || pw)
            })
        }),
        chat_db_size: num("chatdbSize"),
        media_size: num("mediaSize"),
        messages: num("numOfMessages"),
        app_version: meta.get("versionOfAppWhenBackup").and_then(|v| v.as_str()).map(str::to_owned),
        name: raw.name,
    }
}

/// The backup's main message database: `Databases/msgstore.db.crypt15`
/// (or `.crypt14`/`.crypt12`, in backups without end-to-end encryption).
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
    fn safe_backup_name() {
        assert!(is_backup_name("clients/wa/backups/5511999999999"));
        assert!(!is_backup_name("clients/wa/backups/../../x"));
        assert!(!is_backup_name("clients/wa/backups/"));
        assert!(!is_backup_name("https://evil/clients/wa/backups/1"));
    }

    #[test]
    fn backup_metadata() {
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
    fn picks_the_msgstore() {
        let f = |p: &str| BackupFile { name: format!("x/files/{p}"), path: p.into(), size_bytes: 1, md5: None, metadata: None };
        let files = [f("Databases/msgstore-2026-01-01.1.db.crypt15"), f("Databases/msgstore.db.crypt14"), f("Databases/msgstore.db.crypt15"), f("Media/ab.mcrypt1")];
        assert_eq!(find_msgstore(&files).unwrap().path, "Databases/msgstore.db.crypt15");
        assert_eq!(find_msgstore(&files[3..]), None);
    }
}

#[cfg(test)]
mod tests_metadata {
    use super::*;

    fn raw(metadata: &str) -> RawBackup {
        RawBackup {
            name: "clients/wa/backups/5511900000001".into(),
            update_time: None,
            size_bytes: None,
            active_transaction_id: None,
            metadata: Some(metadata.into()),
        }
    }

    #[test]
    fn e2e_in_both_metadata_formats() {
        // Current format (real backup from 2026-11, without E2E).
        let current = r#"{"backupSize":3591962311,"chatdbSize":673041098,"encryptedBackupMetadata":{"isEncryptionKeyEncrypted":false,"isPasswordEncrypted":false,"isPasswordOrEncryptionKeyEncrypted":false},"numOfMessages":841593,"mediaSize":2860451951,"versionOfAppWhenBackup":"2.26.44.90-play-release"}"#;
        let b = parse_backup(raw(current));
        assert_eq!(b.encrypted, Some(false));
        assert_eq!((b.chat_db_size, b.messages), (Some(673041098), Some(841593)));
        let with_key = r#"{"encryptedBackupMetadata":{"isEncryptionKeyEncrypted":true,"isPasswordEncrypted":false}}"#;
        assert_eq!(parse_backup(raw(with_key)).encrypted, Some(true));
        // Old format.
        assert_eq!(parse_backup(raw(r#"{"encryptedBackupEnabled":true}"#)).encrypted, Some(true));
        assert_eq!(parse_backup(raw("{}")).encrypted, None);
    }
}
