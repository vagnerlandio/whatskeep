//! Media in the vault: one encrypted file per media item + a SQLCipher index.
//!
//! ```text
//! stores/<app>-media.db       index: sha256 → blob, path on the phone → sha256
//! stores/<app>-media/<id>     encrypted blob (random id: the name reveals nothing)
//! ```
//!
//! **Blob** (STREAM scheme, Hoang–Reyhanitabar–Rogaway–Vizár):
//!
//! ```text
//! "WAMB" | version 1 | nonce prefix (7) | segment 0 | segment 1 | ...
//! segment i = AES-256-GCM(key, nonce = prefix ‖ i (u32 BE) ‖ last (u8),
//!                         aad = blob id, up to 1 MiB of plaintext) ‖ tag
//! ```
//!
//! * Fixed-size segments allow reading any range (video seeking)
//!   by decrypting only the segments needed.
//! * The "last" byte in the nonce prevents truncating the file; the id in the AAD
//!   prevents swapping one blob for another.
//! * The index is separate from the message database: re-importing the msgstore
//!   does not lose the media. Messages are linked by the `file_hash` that
//!   WhatsApp stores (SHA-256 of the file, base64) or, without it, by the path.

use std::fs::{self, File};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use aes_gcm::aead::{AeadInPlace, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use crate::store::open_keyed;
use crate::{Error, Key, fsutil, random};

const MAGIC: &[u8; 4] = b"WAMB";
const VERSION: u8 = 1;
const HEADER: u64 = 12;
pub const SEGMENT: usize = 1 << 20;
const TAG: usize = 16;

/// Per-file limit (long videos fit; a bigger file is suspicious).
pub const MAX_MEDIA: u64 = 16 << 30;

const DDL: &str = "
CREATE TABLE IF NOT EXISTS blob (
    sha256   TEXT PRIMARY KEY,     -- base64, same as WhatsApp's file_hash
    id       TEXT NOT NULL UNIQUE, -- name of the encrypted file
    size     INTEGER NOT NULL,
    mime     TEXT,
    added_at INTEGER NOT NULL
) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS path (
    path   TEXT PRIMARY KEY,       -- normalized path (Media/...)
    sha256 TEXT NOT NULL REFERENCES blob(sha256)
) WITHOUT ROWID;
";

/// Media path starting at `Media/`, the same for the msgstore
/// (`Media/WhatsApp Images/x.jpg` or `/storage/.../WhatsApp/Media/...`) and for
/// the folder copied from the phone.
pub fn normalize_path(p: &str) -> Option<String> {
    let p = p.replace('\\', "/");
    // The last occurrence: the folder where the user copied the backup may have
    // "Media" in its path (`/Volumes/Media/WhatsApp/Media/...`).
    let i = match p.rfind("/Media/") {
        Some(i) => i + 1,
        None if p.starts_with("Media/") => 0,
        None => return None,
    };
    let out = &p[i..];
    (out.len() > "Media/".len()).then(|| out.to_owned())
}

/// Paths and keys of an app's media. Cloneable to run outside the state lock.
#[derive(Clone)]
pub struct MediaHandle {
    index: PathBuf,
    dir: PathBuf,
    blob_key: Key,
    index_key: Key,
}

impl std::fmt::Debug for MediaHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MediaHandle").field("keys", &"<redacted>").finish()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlobRef {
    pub id: String,
    pub sha256: String,
    pub size: u64,
    pub mime: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Put {
    Added,
    /// The content was already in the vault (only the path was recorded).
    AlreadyStored,
}

impl MediaHandle {
    pub(crate) fn new(stores: &Path, app: &str, blob_key: Key, index_key: Key) -> Self {
        Self {
            index: stores.join(format!("{app}-media.db")),
            dir: stores.join(format!("{app}-media")),
            blob_key,
            index_key,
        }
    }

    pub fn exists(&self) -> bool {
        self.index.is_file()
    }

    /// Deletes the index and every blob.
    pub(crate) fn delete(&self) -> Result<(), Error> {
        fsutil::remove_database(&self.index)?;
        fsutil::remove_if_exists(fs::remove_dir_all(&self.dir))?;
        Ok(())
    }

    /// Index for writing (created if it does not exist).
    pub fn writer(&self) -> Result<MediaWriter, Error> {
        fsutil::create_private_dir(&self.dir)?;
        let created = !self.index.exists();
        let conn = open_keyed(
            &self.index,
            &self.index_key,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
        )?;
        if created {
            fsutil::set_private(&self.index)?;
        }
        // WAL: the viewer reads while an import writes.
        conn.execute_batch("PRAGMA journal_mode = WAL;")?;
        conn.execute_batch(DDL)?;
        Ok(MediaWriter { handle: self.clone(), conn })
    }

    /// Read-only index. `None` if no media has been imported.
    pub fn reader(&self) -> Result<Option<MediaReader>, Error> {
        if !self.exists() {
            return Ok(None);
        }
        let conn = open_keyed(&self.index, &self.index_key, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        conn.execute_batch("PRAGMA query_only = ON;")?;
        Ok(Some(MediaReader { handle: self.clone(), conn }))
    }

    fn blob_path(&self, id: &str) -> PathBuf {
        self.dir.join(id)
    }
}

fn find(conn: &Connection, sha256: Option<&str>, path: Option<&str>) -> Result<Option<BlobRef>, Error> {
    let row = |r: &rusqlite::Row| {
        Ok(BlobRef { sha256: r.get(0)?, id: r.get(1)?, size: r.get::<_, i64>(2)? as u64, mime: r.get(3)? })
    };
    if let Some(h) = sha256 {
        let b = conn
            .prepare_cached("SELECT sha256, id, size, mime FROM blob WHERE sha256 = ?1")?
            .query_row([h], row)
            .optional()?;
        if b.is_some() {
            return Ok(b);
        }
    }
    if let Some(p) = path.and_then(normalize_path) {
        return Ok(conn
            .prepare_cached(
                "SELECT b.sha256, b.id, b.size, b.mime FROM path p JOIN blob b ON b.sha256 = p.sha256 WHERE p.path = ?1",
            )?
            .query_row([p], row)
            .optional()?);
    }
    Ok(None)
}

pub struct MediaWriter {
    handle: MediaHandle,
    conn: Connection,
}

impl MediaWriter {
    pub fn find(&self, sha256: Option<&str>, path: Option<&str>) -> Result<Option<BlobRef>, Error> {
        find(&self.conn, sha256, path)
    }

    /// Encrypts the content of `reader` into a new blob and records it in the index.
    ///
    /// * `expected_sha256`: if given, the content must have this hash
    ///   (otherwise nothing is written and [`Error::Corrupt`] is returned).
    /// * `path`: the media path on the phone, to find messages without `file_hash`.
    pub fn put(
        &self,
        reader: impl Read,
        expected_sha256: Option<&str>,
        mime: Option<&str>,
        path: Option<&str>,
    ) -> Result<(String, Put), Error> {
        let id = hex(&random::<16>()?);
        let tmp = self.handle.dir.join(format!("{id}.tmp"));
        let written = write_blob(&tmp, &self.handle.blob_key, &id, reader);
        let (sha256, size) = match written {
            Ok(v) => v,
            Err(e) => {
                let _ = fs::remove_file(&tmp);
                return Err(e);
            }
        };
        if expected_sha256.is_some_and(|e| e != sha256) {
            let _ = fs::remove_file(&tmp);
            return Err(Error::Corrupt("o conteúdo da mídia não confere com o hash do WhatsApp"));
        }

        let exists: bool =
            self.conn.query_row("SELECT EXISTS(SELECT 1 FROM blob WHERE sha256 = ?1)", [&sha256], |r| r.get(0))?;
        let outcome = if exists {
            let _ = fs::remove_file(&tmp);
            Put::AlreadyStored
        } else {
            let final_path = self.handle.blob_path(&id);
            fs::rename(&tmp, &final_path)?;
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);
            if let Err(e) = self.conn.execute(
                "INSERT INTO blob (sha256, id, size, mime, added_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![sha256, id, size as i64, mime, now],
            ) {
                let _ = fs::remove_file(&final_path);
                return Err(e.into());
            }
            Put::Added
        };
        if let Some(p) = path.and_then(normalize_path) {
            self.conn.execute("INSERT OR IGNORE INTO path (path, sha256) VALUES (?1, ?2)", params![p, sha256])?;
        }
        Ok((sha256, outcome))
    }

    /// Deletes temporary files from interrupted imports.
    pub fn cleanup(&self) {
        let Ok(entries) = fs::read_dir(&self.handle.dir) else { return };
        for e in entries.flatten() {
            if e.file_name().to_string_lossy().ends_with(".tmp") {
                let _ = fs::remove_file(e.path());
            }
        }
    }
}

pub struct MediaReader {
    handle: MediaHandle,
    conn: Connection,
}

impl MediaReader {
    pub fn find(&self, sha256: Option<&str>, path: Option<&str>) -> Result<Option<BlobRef>, Error> {
        find(&self.conn, sha256, path)
    }

    /// Bytes `[start, end)` of the decrypted content.
    pub fn read_range(&self, blob: &BlobRef, start: u64, end: u64) -> Result<Zeroizing<Vec<u8>>, Error> {
        let end = end.min(blob.size);
        let mut out = Zeroizing::new(Vec::with_capacity(end.saturating_sub(start) as usize));
        if start >= end {
            return Ok(out);
        }
        let mut f = File::open(self.handle.blob_path(&blob.id))?;
        let prefix = read_header(&mut f)?;
        let segments = segment_count(blob.size);
        let first = start / SEGMENT as u64;
        let last = (end - 1) / SEGMENT as u64;
        for i in first..=last {
            let plain = read_segment(&mut f, &self.handle.blob_key, &blob.id, &prefix, i, i + 1 == segments)?;
            let seg_start = i * SEGMENT as u64;
            let from = start.saturating_sub(seg_start) as usize;
            let to = ((end - seg_start) as usize).min(plain.len());
            out.extend_from_slice(&plain[from..to]);
        }
        Ok(out)
    }

    /// Writes the whole decrypted content to `out` (exporting a document).
    pub fn copy_to(&self, blob: &BlobRef, mut out: impl Write) -> Result<(), Error> {
        let mut f = File::open(self.handle.blob_path(&blob.id))?;
        let prefix = read_header(&mut f)?;
        let segments = segment_count(blob.size);
        for i in 0..segments {
            let plain = read_segment(&mut f, &self.handle.blob_key, &blob.id, &prefix, i, i + 1 == segments)?;
            out.write_all(&plain)?;
        }
        Ok(())
    }
}

fn segment_count(size: u64) -> u64 {
    size.div_ceil(SEGMENT as u64).max(1)
}

fn nonce(prefix: &[u8; 7], i: u64, last: bool) -> Result<[u8; 12], Error> {
    let i = u32::try_from(i).map_err(|_| Error::Corrupt("mídia grande demais"))?;
    let mut n = [0u8; 12];
    n[..7].copy_from_slice(prefix);
    n[7..11].copy_from_slice(&i.to_be_bytes());
    n[11] = last as u8;
    Ok(n)
}

fn write_blob(path: &Path, key: &Key, id: &str, mut reader: impl Read) -> Result<(String, u64), Error> {
    let cipher = Aes256Gcm::new(key.as_ref().into());
    let prefix: [u8; 7] = random()?;
    let mut f = io::BufWriter::new(fsutil::create_private(path)?);
    f.write_all(MAGIC)?;
    f.write_all(&[VERSION])?;
    f.write_all(&prefix)?;

    let mut sha = Sha256::new();
    let mut size = 0u64;
    let mut current = Zeroizing::new(read_full(&mut reader, SEGMENT)?);
    let mut i = 0u64;
    loop {
        // Read the next one before encrypting the current one, to know whether this is the last.
        let next = if current.len() == SEGMENT { Zeroizing::new(read_full(&mut reader, SEGMENT)?) } else { Zeroizing::new(Vec::new()) };
        let last = next.is_empty();
        sha.update(&*current);
        size += current.len() as u64;
        if size > MAX_MEDIA {
            return Err(Error::Corrupt("mídia acima do limite de tamanho"));
        }
        let mut buf = current;
        cipher
            .encrypt_in_place(Nonce::from_slice(&nonce(&prefix, i, last)?), id.as_bytes(), &mut *buf)
            .map_err(|_| Error::Corrupt("falha ao cifrar a mídia"))?;
        f.write_all(&buf)?;
        if last {
            break;
        }
        current = next;
        i += 1;
    }
    let f = f.into_inner().map_err(|e| e.into_error())?;
    f.sync_all()?;
    Ok((B64.encode(sha.finalize()), size))
}

fn read_full(r: &mut impl Read, n: usize) -> io::Result<Vec<u8>> {
    let mut buf = Vec::with_capacity(n);
    r.take(n as u64).read_to_end(&mut buf)?;
    Ok(buf)
}

fn read_header(f: &mut File) -> Result<[u8; 7], Error> {
    let mut h = [0u8; HEADER as usize];
    f.read_exact(&mut h).map_err(|_| Error::Corrupt("mídia truncada"))?;
    if &h[..4] != MAGIC || h[4] != VERSION {
        return Err(Error::Corrupt("formato de mídia desconhecido"));
    }
    Ok(h[5..12].try_into().expect("7 bytes"))
}

fn read_segment(
    f: &mut File,
    key: &Key,
    id: &str,
    prefix: &[u8; 7],
    i: u64,
    last: bool,
) -> Result<Zeroizing<Vec<u8>>, Error> {
    f.seek(SeekFrom::Start(HEADER + i * (SEGMENT + TAG) as u64))?;
    let mut buf = Zeroizing::new(Vec::with_capacity(SEGMENT + TAG));
    f.take((SEGMENT + TAG) as u64).read_to_end(&mut buf)?;
    if buf.len() < TAG || (!last && buf.len() != SEGMENT + TAG) {
        return Err(Error::Corrupt("mídia truncada"));
    }
    Aes256Gcm::new(key.as_ref().into())
        .decrypt_in_place(Nonce::from_slice(&nonce(prefix, i, last)?), id.as_bytes(), &mut *buf)
        .map_err(|_| Error::Corrupt("mídia adulterada ou corrompida"))?;
    Ok(buf)
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}
