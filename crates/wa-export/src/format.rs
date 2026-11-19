//! Format of the exported package (version 1). The browser (`packages/export-shell`)
//! implements reading with WebCrypto + Argon2id in WASM; this module implements
//! writing and, for the tests, the same reading.
//!
//! ```text
//! password ──Argon2id(salt, m, t, p)──► slot key ──AES-GCM──► { master, chats }
//! master ──HKDF(salt, "wa-export chat v1:" ‖ id)──► chat key (chunks)
//! chat key ──HKDF(salt, "wa-export media v1:" ‖ media id)──► media key
//! ```
//!
//! * The ids (of chats, chunks and media) are random: nothing in the file
//!   names reveals a contact, date or count.
//! * Chunk: `[u32 LE length][gzip(JSON)][zeros up to a multiple of 64 KiB]`,
//!   encrypted with AES-256-GCM (random nonce, AAD = chunk id).
//! * Encrypted media: 1 MiB STREAM segments (nonce = 7-byte prefix ‖ i u32 BE ‖
//!   last, AAD = media id), as in the vault.

use std::io::{Read, Write};

use aes_gcm::aead::{AeadInPlace, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use argon2::{Algorithm, Argon2, Params, Version};
use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use hkdf::Hkdf;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use zeroize::Zeroizing;

use crate::Error;

pub const FORMAT: u32 = 1;
pub const SLOT_AAD: &[u8] = b"wa-export slot v1";
pub const SEGMENT: usize = 1 << 20;
pub const TAG: usize = 16;
/// Chunks are rounded up to multiples of this: the file size does not
/// reveal the exact message volume.
pub const PAD: usize = 64 << 10;

pub type Key = Zeroizing<[u8; 32]>;

/// Argon2id cost in the browser (WASM, single thread): 64 MiB, 3 iterations.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Kdf {
    pub m: u32,
    pub t: u32,
    pub p: u32,
}

impl Kdf {
    pub const DEFAULT: Self = Self { m: 64 * 1024, t: 3, p: 1 };
    /// Tests only.
    pub const TEST: Self = Self { m: 8 * 1024, t: 1, p: 1 };
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Manifest {
    pub format: u32,
    pub kdf: KdfJson,
    pub slots: Vec<Sealed>,
    /// `none`, `encrypted` or `plain`.
    pub media: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct KdfJson {
    pub alg: String,
    pub m: u32,
    pub t: u32,
    pub p: u32,
    pub salt: String,
}

/// Nonce + ciphertext, in base64.
#[derive(Debug, Serialize, Deserialize)]
pub struct Sealed {
    pub n: String,
    pub c: String,
}

pub(crate) fn b64(b: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(b)
}

pub fn unb64(s: &str) -> Result<Vec<u8>, Error> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.decode(s).map_err(|_| Error::Format("base64 inválido"))
}

pub(crate) fn random<const N: usize>() -> Result<[u8; N], Error> {
    let mut b = [0u8; N];
    getrandom::fill(&mut b).map_err(|_| Error::Random)?;
    Ok(b)
}

pub(crate) fn random_id() -> Result<String, Error> {
    Ok(random::<8>()?.iter().map(|b| format!("{b:02x}")).collect())
}

pub fn password_key(password: &str, salt: &[u8], kdf: Kdf) -> Result<Key, Error> {
    let params = Params::new(kdf.m, kdf.t, kdf.p, Some(32)).map_err(|_| Error::Format("parâmetros do KDF"))?;
    let mut out = Key::default();
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(password.as_bytes(), salt, out.as_mut())
        .map_err(|_| Error::Format("Argon2id"))?;
    Ok(out)
}

pub fn derive(ikm: &[u8; 32], salt: &[u8], info: &str) -> Key {
    let mut out = Key::default();
    Hkdf::<Sha256>::new(Some(salt), ikm)
        .expand(info.as_bytes(), out.as_mut())
        .expect("32 bytes fit in HKDF-SHA256");
    out
}

pub fn chat_key(master: &[u8; 32], salt: &[u8], chat_id: &str) -> Key {
    derive(master, salt, &format!("wa-export chat v1:{chat_id}"))
}

pub fn media_key(chat_key: &[u8; 32], salt: &[u8], media_id: &str) -> Key {
    derive(chat_key, salt, &format!("wa-export media v1:{media_id}"))
}

pub fn seal(key: &[u8; 32], aad: &[u8], plain: &[u8]) -> Result<(Vec<u8>, [u8; 12]), Error> {
    let nonce: [u8; 12] = random()?;
    let mut buf = plain.to_vec();
    Aes256Gcm::new(key.into())
        .encrypt_in_place(Nonce::from_slice(&nonce), aad, &mut buf)
        .map_err(|_| Error::Format("falha ao cifrar"))?;
    Ok((buf, nonce))
}

pub fn open(key: &[u8; 32], aad: &[u8], nonce: &[u8], ct: &[u8]) -> Result<Zeroizing<Vec<u8>>, Error> {
    if nonce.len() != 12 {
        return Err(Error::Format("nonce"));
    }
    let mut buf = Zeroizing::new(ct.to_vec());
    Aes256Gcm::new(key.into())
        .decrypt_in_place(Nonce::from_slice(nonce), aad, &mut *buf)
        .map_err(|_| Error::Authentication)?;
    Ok(buf)
}

/// JSON → gzip → `[u32 LE length][gzip][padding]`.
pub fn pack_chunk(json: &[u8]) -> Result<Zeroizing<Vec<u8>>, Error> {
    let mut gz = GzEncoder::new(Vec::new(), Compression::default());
    gz.write_all(json)?;
    let gz = Zeroizing::new(gz.finish()?);
    let len = u32::try_from(gz.len()).map_err(|_| Error::Format("pedaço grande demais"))?;
    let total = (4 + gz.len()).div_ceil(PAD) * PAD;
    let mut out = Zeroizing::new(Vec::with_capacity(total));
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(&gz);
    out.resize(total, 0);
    Ok(out)
}

pub fn unpack_chunk(plain: &[u8]) -> Result<Zeroizing<Vec<u8>>, Error> {
    let len = u32::from_le_bytes(plain.get(..4).ok_or(Error::Format("pedaço curto"))?.try_into().unwrap()) as usize;
    let gz = plain.get(4..4 + len).ok_or(Error::Format("pedaço truncado"))?;
    let mut out = Zeroizing::new(Vec::new());
    GzDecoder::new(gz).read_to_end(&mut out)?;
    Ok(out)
}

pub fn stream_nonce(prefix: &[u8; 7], i: u64, last: bool) -> Result<[u8; 12], Error> {
    let i = u32::try_from(i).map_err(|_| Error::Format("mídia grande demais"))?;
    let mut n = [0u8; 12];
    n[..7].copy_from_slice(prefix);
    n[7..11].copy_from_slice(&i.to_be_bytes());
    n[11] = last as u8;
    Ok(n)
}

/// Encrypted size of a media file with `size` bytes.
pub fn stream_len(size: u64) -> u64 {
    let segs = size.div_ceil(SEGMENT as u64).max(1);
    size + segs * TAG as u64
}

/// Encrypts a media file in segments, writing to `out` as the content arrives.
pub struct StreamWriter<'a, W: Write> {
    out: &'a mut W,
    cipher: Aes256Gcm,
    prefix: [u8; 7],
    aad: Vec<u8>,
    total: u64,
    written: u64,
    index: u64,
    buf: Zeroizing<Vec<u8>>,
}

impl<'a, W: Write> StreamWriter<'a, W> {
    pub fn new(out: &'a mut W, key: &[u8; 32], prefix: [u8; 7], media_id: &str, total: u64) -> Self {
        Self {
            out,
            cipher: Aes256Gcm::new(key.into()),
            prefix,
            aad: media_id.as_bytes().to_vec(),
            total,
            written: 0,
            index: 0,
            buf: Zeroizing::new(Vec::with_capacity(SEGMENT + TAG)),
        }
    }

    fn flush_segment(&mut self) -> std::io::Result<()> {
        let last = self.written >= self.total;
        let nonce = stream_nonce(&self.prefix, self.index, last).map_err(std::io::Error::other)?;
        self.cipher
            .encrypt_in_place(Nonce::from_slice(&nonce), &self.aad, &mut *self.buf)
            .map_err(|_| std::io::Error::other("falha ao cifrar a mídia"))?;
        self.out.write_all(&self.buf)?;
        self.buf.clear();
        self.index += 1;
        Ok(())
    }

    /// Closes the media file. Empty media becomes a single tag-only segment.
    pub fn finish(mut self) -> std::io::Result<()> {
        if self.written != self.total {
            return Err(std::io::Error::other("tamanho da mídia diferente do esperado"));
        }
        if !self.buf.is_empty() || self.index == 0 {
            self.flush_segment()?;
        }
        Ok(())
    }
}

impl<W: Write> Write for StreamWriter<'_, W> {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        let room = SEGMENT - self.buf.len();
        let n = data.len().min(room);
        self.buf.extend_from_slice(&data[..n]);
        self.written += n as u64;
        if self.buf.len() == SEGMENT {
            self.flush_segment()?;
        }
        Ok(n)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Decrypts a whole media file (for the tests; the browser does the same in JS).
pub fn open_stream(key: &[u8; 32], prefix: &[u8; 7], media_id: &str, ct: &[u8], size: u64) -> Result<Vec<u8>, Error> {
    let cipher = Aes256Gcm::new(key.into());
    let segs = size.div_ceil(SEGMENT as u64).max(1);
    let mut out = Vec::with_capacity(size as usize);
    let mut off = 0usize;
    for i in 0..segs {
        let plain_len = (size - i * SEGMENT as u64).min(SEGMENT as u64) as usize;
        let mut seg = ct.get(off..off + plain_len + TAG).ok_or(Error::Format("mídia truncada"))?.to_vec();
        cipher
            .decrypt_in_place(Nonce::from_slice(&stream_nonce(prefix, i, i + 1 == segs)?), media_id.as_bytes(), &mut seg)
            .map_err(|_| Error::Authentication)?;
        out.extend_from_slice(&seg);
        off += plain_len + TAG;
    }
    Ok(out)
}
