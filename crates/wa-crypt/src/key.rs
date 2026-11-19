use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use crate::Error;

type HmacSha256 = Hmac<Sha256>;

/// The kind of backup the key opens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyKind {
    /// "WhatsApp-provided" key (the `key` file, requires root on Android).
    Crypt14,
    /// E2E backup key: the 64 digits or the `encrypted_backup.key`.
    Crypt15,
}

/// Ready-to-use AES-256 key, wiped from memory on drop.
///
/// For crypt15 it also keeps the root key: Google Drive `.mcrypt1` media
/// have a per-file key derived from it. None of this leaves the crate.
pub struct BackupKey {
    kind: KeyKind,
    aes: Zeroizing<[u8; 32]>,
    root: Option<Zeroizing<[u8; 32]>>,
}

impl std::fmt::Debug for BackupKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BackupKey")
            .field("kind", &self.kind)
            .field("aes", &"<redacted>")
            .finish()
    }
}

/// Exact header of a Java-serialized `byte[]`
/// (`ObjectOutputStream.writeObject(byte[])`), without the 4 length bytes.
const JAVA_BYTE_ARRAY_HEADER: [u8; 23] = [
    0xAC, 0xED, 0x00, 0x05, // STREAM_MAGIC, STREAM_VERSION
    0x75, // TC_ARRAY
    0x72, // TC_CLASSDESC
    0x00, 0x02, b'[', b'B', // class name "[B"
    0xAC, 0xF3, 0x17, 0xF8, 0x06, 0x08, 0x54, 0xE0, // serialVersionUID of byte[]
    0x02, // SC_SERIALIZABLE
    0x00, 0x00, // 0 fields
    0x78, // TC_ENDBLOCKDATA
    0x70, // TC_NULL (no superclass)
];

const CRYPT14_KEY_LEN: usize = 131;

impl BackupKey {
    /// Builds the key from the 64 hexadecimal digits WhatsApp shows when
    /// E2E backup is enabled. Spaces, line breaks and hyphens are ignored.
    pub fn from_hex(input: &str) -> Result<Self, Error> {
        let mut root = Zeroizing::new([0u8; 32]);
        let mut n = 0usize;
        let mut high: Option<u8> = None;
        for c in input.chars() {
            if c.is_whitespace() || c == '-' {
                continue;
            }
            let v = c
                .to_digit(16)
                .ok_or(Error::InvalidKey("a chave só pode conter 0-9 e a-f"))? as u8;
            match high.take() {
                None => high = Some(v),
                Some(h) => {
                    if n == 32 {
                        return Err(Error::InvalidKey("a chave tem mais de 64 dígitos"));
                    }
                    root[n] = (h << 4) | v;
                    n += 1;
                }
            }
        }
        if n != 32 || high.is_some() {
            return Err(Error::InvalidKey("a chave deve ter exatamente 64 dígitos"));
        }
        Ok(Self::from_root_key(&root))
    }

    /// Builds the key from the 32-byte crypt15 root key.
    pub fn from_root_key(root: &[u8; 32]) -> Self {
        Self {
            kind: KeyKind::Crypt15,
            aes: derive_crypt15(root),
            root: Some(Zeroizing::new(*root)),
        }
    }

    /// Reads a key file: `key` (crypt14, 158 bytes) or
    /// `encrypted_backup.key` (crypt15, 59 bytes). Also accepts the raw payload,
    /// without the Java serialization.
    pub fn from_key_file(data: &[u8]) -> Result<Self, Error> {
        let payload = unwrap_java_byte_array(data)?;
        match payload.len() {
            32 => {
                let mut root = Zeroizing::new([0u8; 32]);
                root.copy_from_slice(payload);
                Ok(Self::from_root_key(&root))
            }
            CRYPT14_KEY_LEN => parse_crypt14(payload),
            _ => Err(Error::UnknownKeyFile),
        }
    }

    pub fn kind(&self) -> KeyKind {
        self.kind
    }

    pub(crate) fn aes_key(&self) -> &[u8; 32] {
        &self.aes
    }

    pub(crate) fn root(&self) -> Option<&[u8; 32]> {
        self.root.as_deref()
    }
}

/// HKDF-SHA256 (salt = 32 zeros, info = "backup encryption", L = 32).
/// Equivalent to wa-crypt-tools' `encryptionloop` with a single block.
fn derive_crypt15(root: &[u8; 32]) -> Zeroizing<[u8; 32]> {
    let mut extract = HmacSha256::new_from_slice(&[0u8; 32]).expect("HMAC accepts any key length");
    extract.update(root);
    let prk = Zeroizing::new(<[u8; 32]>::from(extract.finalize().into_bytes()));

    let mut expand = HmacSha256::new_from_slice(&*prk).expect("HMAC accepts any key length");
    expand.update(b"backup encryption");
    expand.update(&[0x01]);
    Zeroizing::new(<[u8; 32]>::from(expand.finalize().into_bytes()))
}

/// Payload layout of the crypt14 `key`:
/// `[0..2]` cipher_version = 00 01, `[2]` key_version ∈ {1,2,3},
/// `[3..35]` server_salt, `[35..51]` google_id, `[51..83]` sha256(google_id),
/// `[83..99]` zeros, `[99..131]` AES key.
fn parse_crypt14(p: &[u8]) -> Result<BackupKey, Error> {
    if p[0..2] != [0x00, 0x01] {
        return Err(Error::InvalidKey("cipher_version desconhecida no arquivo key"));
    }
    if !matches!(p[2], 1..=3) {
        return Err(Error::InvalidKey("key_version desconhecida no arquivo key"));
    }
    if Sha256::digest(&p[35..51])[..] != p[51..83] {
        return Err(Error::InvalidKey("arquivo key corrompido (hash do google_id não confere)"));
    }
    if p[83..99].iter().any(|&b| b != 0) {
        return Err(Error::InvalidKey("arquivo key corrompido (padding não é zero)"));
    }
    let mut aes = Zeroizing::new([0u8; 32]);
    aes.copy_from_slice(&p[99..131]);
    Ok(BackupKey {
        kind: KeyKind::Crypt14,
        aes,
        root: None,
    })
}

fn unwrap_java_byte_array(data: &[u8]) -> Result<&[u8], Error> {
    if !data.starts_with(&[0xAC, 0xED]) {
        return Ok(data);
    }
    let rest = data
        .strip_prefix(&JAVA_BYTE_ARRAY_HEADER[..])
        .ok_or(Error::UnknownKeyFile)?;
    if rest.len() < 4 {
        return Err(Error::UnknownKeyFile);
    }
    let len = u32::from_be_bytes([rest[0], rest[1], rest[2], rest[3]]) as usize;
    let payload = &rest[4..];
    if payload.len() != len {
        return Err(Error::UnknownKeyFile);
    }
    Ok(payload)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_accepts_spaces_and_rejects_garbage() {
        let hex = "00112233 44556677 8899aabb ccddeeff\n00112233-44556677-8899AABB-CCDDEEFF";
        assert!(BackupKey::from_hex(hex).is_ok());
        assert!(matches!(BackupKey::from_hex("abc"), Err(Error::InvalidKey(_))));
        assert!(matches!(BackupKey::from_hex(&"g".repeat(64)), Err(Error::InvalidKey(_))));
        assert!(matches!(BackupKey::from_hex(&"0".repeat(66)), Err(Error::InvalidKey(_))));
        assert!(matches!(BackupKey::from_hex(&"0".repeat(63)), Err(Error::InvalidKey(_))));
    }

    #[test]
    fn debug_does_not_leak_key() {
        let k = BackupKey::from_hex(&"ab".repeat(32)).unwrap();
        let s = format!("{k:?}");
        assert!(s.contains("redacted"));
        assert!(!s.to_lowercase().contains("abab"));
    }

    #[test]
    fn key_file_with_wrong_length() {
        assert!(matches!(
            BackupKey::from_key_file(&[0u8; 40]),
            Err(Error::UnknownKeyFile)
        ));
        let mut java = JAVA_BYTE_ARRAY_HEADER.to_vec();
        java.extend_from_slice(&100u32.to_be_bytes());
        java.extend_from_slice(&[0u8; 10]);
        assert!(matches!(BackupKey::from_key_file(&java), Err(Error::UnknownKeyFile)));
    }
}
