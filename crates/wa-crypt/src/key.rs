use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use crate::Error;

type HmacSha256 = Hmac<Sha256>;

/// O tipo de backup que a chave abre.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyKind {
    /// Chave "fornecida pelo WhatsApp" (arquivo `key`, exige root no Android).
    Crypt14,
    /// Chave do backup E2E: os 64 dígitos ou o `encrypted_backup.key`.
    Crypt15,
}

/// Chave AES-256 pronta para uso, zerada da memória no drop.
///
/// Só guarda a chave AES já derivada, nunca a root key de 64 dígitos.
pub struct BackupKey {
    kind: KeyKind,
    aes: Zeroizing<[u8; 32]>,
}

impl std::fmt::Debug for BackupKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BackupKey")
            .field("kind", &self.kind)
            .field("aes", &"<redacted>")
            .finish()
    }
}

/// Cabeçalho exato de um `byte[]` serializado em Java
/// (`ObjectOutputStream.writeObject(byte[])`), sem os 4 bytes de tamanho.
const JAVA_BYTE_ARRAY_HEADER: [u8; 23] = [
    0xAC, 0xED, 0x00, 0x05, // STREAM_MAGIC, STREAM_VERSION
    0x75, // TC_ARRAY
    0x72, // TC_CLASSDESC
    0x00, 0x02, b'[', b'B', // nome da classe "[B"
    0xAC, 0xF3, 0x17, 0xF8, 0x06, 0x08, 0x54, 0xE0, // serialVersionUID de byte[]
    0x02, // SC_SERIALIZABLE
    0x00, 0x00, // 0 campos
    0x78, // TC_ENDBLOCKDATA
    0x70, // TC_NULL (sem superclasse)
];

const CRYPT14_KEY_LEN: usize = 131;

impl BackupKey {
    /// Monta a chave a partir dos 64 dígitos hexadecimais que o WhatsApp mostra
    /// ao ativar o backup E2E. Espaços, quebras de linha e hífens são ignorados.
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

    /// Monta a chave a partir da root key de 32 bytes do crypt15.
    pub fn from_root_key(root: &[u8; 32]) -> Self {
        Self {
            kind: KeyKind::Crypt15,
            aes: derive_crypt15(root),
        }
    }

    /// Lê um arquivo de chave: `key` (crypt14, 158 bytes) ou
    /// `encrypted_backup.key` (crypt15, 59 bytes). Aceita também o payload cru,
    /// sem a serialização Java.
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
}

/// HKDF-SHA256 (salt = 32 zeros, info = "backup encryption", L = 32).
/// Equivale ao `encryptionloop` do wa-crypt-tools com um único bloco.
fn derive_crypt15(root: &[u8; 32]) -> Zeroizing<[u8; 32]> {
    let mut extract = HmacSha256::new_from_slice(&[0u8; 32]).expect("HMAC aceita qualquer tamanho");
    extract.update(root);
    let prk = Zeroizing::new(<[u8; 32]>::from(extract.finalize().into_bytes()));

    let mut expand = HmacSha256::new_from_slice(&*prk).expect("HMAC aceita qualquer tamanho");
    expand.update(b"backup encryption");
    expand.update(&[0x01]);
    Zeroizing::new(<[u8; 32]>::from(expand.finalize().into_bytes()))
}

/// Layout do payload do `key` do crypt14:
/// `[0..2]` cipher_version = 00 01, `[2]` key_version ∈ {1,2,3},
/// `[3..35]` server_salt, `[35..51]` google_id, `[51..83]` sha256(google_id),
/// `[83..99]` zeros, `[99..131]` chave AES.
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
    fn hex_aceita_espacos_e_rejeita_lixo() {
        let hex = "00112233 44556677 8899aabb ccddeeff\n00112233-44556677-8899AABB-CCDDEEFF";
        assert!(BackupKey::from_hex(hex).is_ok());
        assert!(matches!(BackupKey::from_hex("abc"), Err(Error::InvalidKey(_))));
        assert!(matches!(BackupKey::from_hex(&"g".repeat(64)), Err(Error::InvalidKey(_))));
        assert!(matches!(BackupKey::from_hex(&"0".repeat(66)), Err(Error::InvalidKey(_))));
        assert!(matches!(BackupKey::from_hex(&"0".repeat(63)), Err(Error::InvalidKey(_))));
    }

    #[test]
    fn debug_nao_vaza_chave() {
        let k = BackupKey::from_hex(&"ab".repeat(32)).unwrap();
        let s = format!("{k:?}");
        assert!(s.contains("redacted"));
        assert!(!s.to_lowercase().contains("abab"));
    }

    #[test]
    fn arquivo_de_chave_com_tamanho_errado() {
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
