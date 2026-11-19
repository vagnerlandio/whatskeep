use std::io::Read;

use aes::Aes256;
use aes_gcm::aead::consts::U16;
use aes_gcm::aead::generic_array::GenericArray;
use aes_gcm::{AeadInPlace, AesGcm, KeyInit};
use md5::{Digest, Md5};
use zeroize::Zeroizing;

use crate::{parse_prefix, BackupKey, CryptVersion, Error, KeyKind};

/// AES-256-GCM with a 16-byte nonce, as WhatsApp uses it.
type WaGcm = AesGcm<Aes256, U16>;

/// Safety limit against a "zip bomb" during decompression (8 GiB).
const MAX_INFLATED: u64 = 8 << 30;

const SQLITE_MAGIC: &[u8] = b"SQLite format 3\0";
const ZIP_MAGIC: &[u8] = b"PK\x03\x04";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Payload {
    /// SQLite database (`msgstore.db`, `wa.db`...).
    Sqlite,
    /// Zip (multi-file sticker/wallpaper backups).
    Zip,
    /// Anything else (for example, media).
    Other,
}

/// Decryption result. The bytes are zeroed on drop.
pub struct Decrypted {
    pub version: CryptVersion,
    /// `true` if the file had no trailing md5 (multi-file backup).
    pub multifile: bool,
    pub payload: Payload,
    pub data: Zeroizing<Vec<u8>>,
}

impl std::fmt::Debug for Decrypted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Decrypted")
            .field("version", &self.version)
            .field("multifile", &self.multifile)
            .field("payload", &self.payload)
            .field("len", &self.data.len())
            .finish()
    }
}

/// Decrypts a whole `.crypt14`/`.crypt15` in memory and inflates the zlib.
///
/// The GCM tag is verified **before** any byte is released. With a wrong key
/// or a tampered file, returns [`Error::Authentication`] with no partial plaintext.
pub fn decrypt(file: &[u8], key: &BackupKey) -> Result<Decrypted, Error> {
    let prefix = parse_prefix(file)?;
    let expected = match prefix.version {
        CryptVersion::Crypt14 => KeyKind::Crypt14,
        CryptVersion::Crypt15 => KeyKind::Crypt15,
    };
    if key.kind() != expected {
        return Err(Error::KeyMismatch {
            key: key.kind(),
            file: prefix.version,
        });
    }

    let body = &file[prefix.header_len..];
    if body.len() < 32 {
        return Err(Error::Truncated);
    }

    // With trailing md5: [ct][tag][md5]. Without md5 (multi-file): [ct][tag].
    let n = file.len();
    let has_checksum = Md5::digest(&file[..n - 16])[..] == file[n - 16..];
    let sealed = if has_checksum {
        &body[..body.len() - 16]
    } else {
        body
    };
    let (ciphertext, tag) = sealed.split_at(sealed.len() - 16);

    let cipher = WaGcm::new(GenericArray::from_slice(key.aes_key()));
    let mut buf = Zeroizing::new(ciphertext.to_vec());
    cipher
        .decrypt_in_place_detached(
            GenericArray::from_slice(&prefix.iv),
            b"",
            &mut buf,
            GenericArray::from_slice(tag),
        )
        .map_err(|_| Error::Authentication)?;

    let data = if is_zlib(&buf) { inflate(&buf)? } else { buf };
    let payload = if data.starts_with(SQLITE_MAGIC) {
        Payload::Sqlite
    } else if data.starts_with(ZIP_MAGIC) {
        Payload::Zip
    } else {
        Payload::Other
    };

    Ok(Decrypted {
        version: prefix.version,
        multifile: !has_checksum,
        payload,
        data,
    })
}

fn is_zlib(b: &[u8]) -> bool {
    b.len() >= 2 && b[0] & 0x0F == 8 && (u16::from(b[0]) << 8 | u16::from(b[1])) % 31 == 0
}

/// Inflates without leaving copies of the plaintext in memory: each reallocation
/// swaps the buffer for a bigger one, and the old one (a `Zeroizing`) is zeroed on drop.
fn inflate(compressed: &[u8]) -> Result<Zeroizing<Vec<u8>>, Error> {
    let mut decoder = flate2::read::ZlibDecoder::new(compressed);
    let mut out = Zeroizing::new(Vec::with_capacity(compressed.len().saturating_mul(3)));
    let mut chunk = Zeroizing::new([0u8; 64 * 1024]);
    loop {
        let read = decoder.read(&mut chunk[..]).map_err(Error::Decompress)?;
        if read == 0 {
            break;
        }
        if out.len() as u64 + read as u64 > MAX_INFLATED {
            return Err(Error::TooLarge(MAX_INFLATED));
        }
        if out.len() + read > out.capacity() {
            let new_cap = (out.capacity() * 2).max(out.len() + read);
            let mut bigger = Zeroizing::new(Vec::with_capacity(new_cap));
            bigger.extend_from_slice(&out);
            out = bigger; // the old buffer is zeroed here
        }
        out.extend_from_slice(&chunk[..read]);
    }
    Ok(out)
}
