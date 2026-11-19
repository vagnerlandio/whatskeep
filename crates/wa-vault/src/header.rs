//! `vault.json`: parâmetros do Argon2id e a chave de dados (DEK) embrulhada.
//!
//! ```text
//! senha ──Argon2id(salt, m, t, p)──► KEK ──AES-256-GCM(nonce, AAD)──► DEK embrulhada
//! ```
//!
//! A DEK é aleatória e nunca muda. Trocar a senha só reembrulha a DEK, sem
//! recifrar os bancos. O AAD amarra o formato, o id do cofre e os parâmetros
//! do KDF: editar o arquivo para baixar o custo do Argon2id faz a abertura falhar.

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

/// Custo do Argon2id. Fica gravado no cabeçalho, então pode mudar entre versões
/// do app sem quebrar cofres antigos.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KdfParams {
    /// Memória em KiB.
    pub m_kib: u32,
    /// Iterações.
    pub t: u32,
    /// Paralelismo.
    pub p: u32,
}

impl KdfParams {
    /// 256 MiB, 3 iterações, 4 lanes: ~0,5 s num Mac Apple Silicon, 1–2 s numa máquina modesta.
    pub const DEFAULT: Self = Self { m_kib: 256 * 1024, t: 3, p: 4 };

    /// O mínimo aceito. Só para testes: não use num cofre real.
    pub const MINIMUM: Self = Self { m_kib: 8 * 1024, t: 1, p: 1 };

    /// Limites para um cabeçalho adulterado não travar a máquina (nem baixar o custo).
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
    /// Cabeçalho novo, com id de cofre aleatório.
    pub fn create(password: &str, params: KdfParams, dek: &Key) -> Result<Self, Error> {
        let vault_id: [u8; 16] = random()?;
        Self::wrap(B64.encode(vault_id), password, params, dek)
    }

    /// Mesmo cofre (mesmo id e mesma DEK), senha nova e salt novo.
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

    /// Deriva a KEK da senha e desembrulha a DEK. Senha errada e cabeçalho
    /// adulterado dão o mesmo erro: a tag GCM não confere.
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

    /// Id aleatório do cofre. Entra como salt do HKDF das chaves dos bancos.
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

    /// Grava em arquivo temporário, faz fsync e renomeia: uma queda de energia
    /// deixa o cabeçalho antigo ou o novo, nunca um pela metade.
    pub fn write_atomic(&self, path: &Path) -> Result<(), Error> {
        let json = serde_json::to_vec_pretty(self).expect("cabeçalho serializável");
        let tmp = path.with_extension("json.tmp");
        {
            let mut f = crate::fsutil::create_private(&tmp)?;
            f.write_all(&json)?;
            f.sync_all()?;
        }
        fs::rename(&tmp, path)?;
        if let Some(dir) = path.parent() {
            // fsync do diretório para a renomeação ser durável (no-op no Windows).
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
