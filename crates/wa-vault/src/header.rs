//! `vault.json`: Argon2id parameters and the wrapped data key (DEK).
//!
//! ```text
//! password ──Argon2id(salt, m, t, p)──► KEK ──AES-256-GCM(nonce, AAD)──► wrapped DEK
//! ```
//!
//! The DEK is random and never changes. Changing the password only rewraps the DEK,
//! without re-encrypting the databases. The AAD binds the format, the vault id and the
//! KDF parameters: editing the file to lower the Argon2id cost makes unlocking fail.

use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

use aes_gcm::aead::{AeadInPlace, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use argon2::{Algorithm, Argon2, Params, Version};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::{Error, Key, random};

const FORMAT: u32 = 1;
const ALG: &str = "argon2id-v19";

/// Argon2id cost. It is stored in the header, so it can change between app
/// versions without breaking old vaults.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KdfParams {
    /// Memory in KiB.
    pub m_kib: u32,
    /// Iterations.
    pub t: u32,
    /// Parallelism.
    pub p: u32,
}

impl KdfParams {
    /// 256 MiB, 3 iterations, 4 lanes: ~0.5 s on an Apple Silicon Mac, 1–2 s on a modest machine.
    pub const DEFAULT: Self = Self { m_kib: 256 * 1024, t: 3, p: 4 };

    /// The minimum accepted. For tests only: do not use it in a real vault.
    pub const MINIMUM: Self = Self { m_kib: 8 * 1024, t: 1, p: 1 };

    /// Limits so a tampered header cannot freeze the machine (nor lower the cost).
    fn validate(self) -> Result<Self, Error> {
        let ok = (Self::MINIMUM.m_kib..=4 * 1024 * 1024).contains(&self.m_kib)
            && (1..=64).contains(&self.t)
            && (1..=16).contains(&self.p);
        if ok { Ok(self) } else { Err(Error::Corrupt("parâmetros do KDF fora dos limites")) }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Header {
    format: u32,
    vault_id: String,
    kdf: Kdf,
    wrapped_key: Wrapped,
}

#[derive(Clone, Serialize, Deserialize)]
struct Kdf {
    alg: String,
    m_kib: u32,
    t: u32,
    p: u32,
    salt: String,
}

#[derive(Clone, Serialize, Deserialize)]
struct Wrapped {
    nonce: String,
    ct: String,
}

impl Header {
    /// New header, with a random vault id.
    pub fn create(password: &str, params: KdfParams, dek: &Key) -> Result<Self, Error> {
        let vault_id: [u8; 16] = random()?;
        Self::wrap(B64.encode(vault_id), password, params, dek)
    }

    /// Same vault (same id and same DEK), new password and new salt.
    pub fn rewrap(&self, password: &str, params: KdfParams, dek: &Key) -> Result<Self, Error> {
        Self::wrap(self.vault_id.clone(), password, params, dek)
    }

    fn wrap(vault_id: String, password: &str, params: KdfParams, dek: &Key) -> Result<Self, Error> {
        let params = params.validate()?;
        let salt: [u8; 16] = random()?;
        let nonce: [u8; 12] = random()?;
        let kdf = Kdf {
            alg: ALG.to_owned(),
            m_kib: params.m_kib,
            t: params.t,
            p: params.p,
            salt: B64.encode(salt),
        };
        let aad = aad(FORMAT, &vault_id, &kdf);
        let kek = derive_kek(password, &salt, params)?;

        let mut buf = Zeroizing::new(dek.to_vec());
        Aes256Gcm::new(kek.as_ref().into())
            .encrypt_in_place(Nonce::from_slice(&nonce), aad.as_bytes(), &mut *buf)
            .map_err(|_| Error::Corrupt("falha ao cifrar a chave"))?;

        Ok(Self {
            format: FORMAT,
            vault_id,
            kdf,
            wrapped_key: Wrapped { nonce: B64.encode(nonce), ct: B64.encode(&*buf) },
        })
    }

    /// Derives the KEK from the password and unwraps the DEK. A wrong password and a
    /// tampered header give the same error: the GCM tag does not match.
    pub fn unwrap_key(&self, password: &str) -> Result<Key, Error> {
        if self.format != FORMAT || self.kdf.alg != ALG {
            return Err(Error::Unsupported);
        }
        let params = KdfParams { m_kib: self.kdf.m_kib, t: self.kdf.t, p: self.kdf.p }.validate()?;
        let salt = decode(&self.kdf.salt)?;
        let nonce = decode(&self.wrapped_key.nonce)?;
        let ct = decode(&self.wrapped_key.ct)?;
        if salt.len() != 16 || nonce.len() != 12 || ct.len() != 32 + 16 {
            return Err(Error::Corrupt("tamanhos inválidos no cabeçalho"));
        }

        let kek = derive_kek(password, &salt, params)?;
        let mut buf = Zeroizing::new(ct);
        Aes256Gcm::new(kek.as_ref().into())
            .decrypt_in_place(
                Nonce::from_slice(&nonce),
                aad(self.format, &self.vault_id, &self.kdf).as_bytes(),
                &mut *buf,
            )
            .map_err(|_| Error::WrongPassword)?;

        let mut dek = Key::default();
        dek.copy_from_slice(&buf);
        Ok(dek)
    }

    /// Random vault id. Used as the HKDF salt for the database keys.
    pub fn vault_id(&self) -> Result<Vec<u8>, Error> {
        decode(&self.vault_id)
    }

    pub fn params(&self) -> KdfParams {
        KdfParams { m_kib: self.kdf.m_kib, t: self.kdf.t, p: self.kdf.p }
    }

    pub fn read(path: &Path) -> Result<Self, Error> {
        let bytes = fs::read(path)?;
        serde_json::from_slice(&bytes).map_err(|_| Error::Corrupt("vault.json ilegível"))
    }

    /// Writes to a temporary file, fsyncs and renames: a power loss leaves
    /// either the old header or the new one, never a half-written one.
    pub fn write_atomic(&self, path: &Path) -> Result<(), Error> {
        let json = serde_json::to_vec_pretty(self).expect("header is serializable");
        let tmp = path.with_extension("json.tmp");
        {
            let mut f = crate::fsutil::create_private(&tmp)?;
            f.write_all(&json)?;
            f.sync_all()?;
        }
        fs::rename(&tmp, path)?;
        if let Some(dir) = path.parent() {
            // fsync the directory so the rename is durable (no-op on Windows).
            let _ = File::open(dir).and_then(|d| d.sync_all());
        }
        Ok(())
    }
}

fn aad(format: u32, vault_id: &str, kdf: &Kdf) -> String {
    format!(
        "wa-vault|{format}|{vault_id}|{}|{}|{}|{}|{}",
        kdf.alg, kdf.m_kib, kdf.t, kdf.p, kdf.salt
    )
}

fn decode(s: &str) -> Result<Vec<u8>, Error> {
    B64.decode(s).map_err(|_| Error::Corrupt("base64 inválido no cabeçalho"))
}

fn derive_kek(password: &str, salt: &[u8], params: KdfParams) -> Result<Key, Error> {
    let params = Params::new(params.m_kib, params.t, params.p, Some(32))
        .map_err(|_| Error::Corrupt("parâmetros do KDF inválidos"))?;
    let mut kek = Key::default();
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(password.as_bytes(), salt, kek.as_mut())
        .map_err(|_| Error::Corrupt("falha no Argon2id"))?;
    Ok(kek)
}
