//! Cofre cifrado onde o histórico importado fica guardado.
//!
//! ```text
//! <dir>/
//!   vault.json                parâmetros do Argon2id + DEK embrulhada (AES-256-GCM)
//!   .metadata_never_index     Spotlight não indexa
//!   stores/whatsapp.db        SQLCipher, chave = HKDF(DEK, id do cofre, "wa-vault store v1:whatsapp")
//!   stores/business.db        SQLCipher, chave = HKDF(DEK, id do cofre, "wa-vault store v1:business")
//! ```
//!
//! * A senha do app só serve para desembrulhar a DEK. Sem ela, nada no disco
//!   é legível, e não existe recuperação.
//! * O [`Vault`] só existe destravado: travar é descartá-lo, e a DEK é zerada no drop.
//! * Pastas ficam com permissão 0700 e arquivos com 0600 (Unix).

mod fsutil;
mod header;
pub mod media;
pub mod query;
mod store;

use std::fs;
use std::path::{Path, PathBuf};

use hkdf::Hkdf;
use sha2::Sha256;
use wa_import::SourceApp;
use zeroize::Zeroizing;

pub use header::KdfParams;
pub use media::{BlobRef, MediaHandle, MediaReader, MediaWriter, Put};
pub use store::{StoreHandle, StoreInfo};

use header::Header;

/// Chave simétrica de 256 bits, zerada no drop.
pub(crate) type Key = Zeroizing<[u8; 32]>;

const HEADER_FILE: &str = "vault.json";
const STORES_DIR: &str = "stores";

pub const MIN_PASSWORD_CHARS: usize = 8;
const MAX_PASSWORD_BYTES: usize = 1024;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("a senha precisa ter pelo menos {MIN_PASSWORD_CHARS} caracteres")]
    WeakPassword,
    #[error("senha incorreta")]
    WrongPassword,
    #[error("já existe um cofre nesta pasta")]
    AlreadyExists,
    #[error("nenhum cofre encontrado")]
    NotFound,
    #[error("este cofre foi criado por uma versão mais nova do app")]
    Unsupported,
    #[error("cofre corrompido: {0}")]
    Corrupt(&'static str),
    #[error("não foi possível abrir o banco do cofre (chave não confere ou arquivo corrompido)")]
    StoreUnreadable,
    #[error("o sistema não forneceu números aleatórios")]
    Random,
    #[error("erro de arquivo: {0}")]
    Io(#[from] std::io::Error),
    #[error("erro no banco de dados: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    Import(#[from] wa_import::Error),
}

/// Cofre destravado. Travar é dar `drop`.
pub struct Vault {
    dir: PathBuf,
    header: Header,
    dek: Key,
}

impl std::fmt::Debug for Vault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Vault").field("dir", &self.dir).field("dek", &"<redacted>").finish()
    }
}

impl Vault {
    pub fn exists(dir: &Path) -> bool {
        dir.join(HEADER_FILE).is_file()
    }

    /// Cria um cofre novo e já o devolve destravado.
    ///
    /// Recusa se já houver cabeçalho ou bancos na pasta: bancos sem o
    /// cabeçalho original são ilegíveis, mas apagá-los é decisão do usuário.
    pub fn create(dir: &Path, password: &str, params: KdfParams) -> Result<Self, Error> {
        check_password(password)?;
        let stores = dir.join(STORES_DIR);
        let has_stores = fs::read_dir(&stores).map(|mut d| d.next().is_some()).unwrap_or(false);
        if Self::exists(dir) || has_stores {
            return Err(Error::AlreadyExists);
        }

        fsutil::create_private_dir(dir)?;
        fsutil::create_private_dir(&stores)?;
        fsutil::mark_no_index(dir)?;
        // Os dados já são cifrados; fora do Time Machine, nem a versão cifrada
        // fica espalhada em discos de backup.
        let _ = fsutil::exclude_from_backup(dir);

        // Direto no buffer zerável, sem cópia intermediária na pilha.
        let mut dek = Key::default();
        getrandom::fill(dek.as_mut()).map_err(|_| Error::Random)?;
        let header = Header::create(password, params, &dek)?;
        header.write_atomic(&dir.join(HEADER_FILE))?;
        Ok(Self { dir: dir.to_owned(), header, dek })
    }

    /// Destrava com a senha. Custa um Argon2id (~0,5 s com os parâmetros padrão num Mac recente).
    pub fn unlock(dir: &Path, password: &str) -> Result<Self, Error> {
        let path = dir.join(HEADER_FILE);
        if !path.is_file() {
            return Err(Error::NotFound);
        }
        let header = Header::read(&path)?;
        let dek = header.unwrap_key(password)?;
        let vault = Self { dir: dir.to_owned(), header, dek };
        vault.cleanup_partial_imports();
        // Idempotente: cobre cofres criados antes desta proteção.
        let _ = fsutil::exclude_from_backup(dir);
        Ok(vault)
    }

    /// Troca a senha. Pede a senha atual de novo para que alguém diante de um
    /// app destravado não consiga trocá-la.
    pub fn change_password(&mut self, current: &str, new: &str, params: KdfParams) -> Result<(), Error> {
        check_password(new)?;
        let check = self.header.unwrap_key(current)?;
        if *check != *self.dek {
            return Err(Error::WrongPassword);
        }
        let header = self.header.rewrap(new, params, &self.dek)?;
        header.write_atomic(&self.dir.join(HEADER_FILE))?;
        self.header = header;
        Ok(())
    }

    /// Parâmetros do Argon2id em uso.
    pub fn kdf_params(&self) -> KdfParams {
        self.header.params()
    }

    pub fn store(&self, app: SourceApp) -> Result<StoreHandle, Error> {
        let vault_id = self.header.vault_id()?;
        let mut key = Key::default();
        Hkdf::<Sha256>::new(Some(&vault_id), self.dek.as_ref())
            .expand(format!("wa-vault store v1:{}", app.as_str()).as_bytes(), key.as_mut())
            .expect("32 bytes cabem no HKDF-SHA256");
        Ok(StoreHandle::new(app, &self.dir.join(STORES_DIR), key))
    }

    /// Mídias de um app: blobs cifrados + índice, com chaves próprias.
    pub fn media(&self, app: SourceApp) -> Result<MediaHandle, Error> {
        let derive = |label: &str| -> Result<Key, Error> {
            let mut key = Key::default();
            Hkdf::<Sha256>::new(Some(&self.header.vault_id()?), self.dek.as_ref())
                .expand(format!("wa-vault {label} v1:{}", app.as_str()).as_bytes(), key.as_mut())
                .expect("32 bytes cabem no HKDF-SHA256");
            Ok(key)
        };
        Ok(MediaHandle::new(&self.dir.join(STORES_DIR), app.as_str(), derive("media")?, derive("media-index")?))
    }

    /// Bancos já importados, com contagens.
    pub fn stores(&self) -> Result<Vec<StoreInfo>, Error> {
        let mut out = Vec::new();
        for app in SourceApp::ALL {
            if let Some(info) = self.store(app)?.info()? {
                out.push(info);
            }
        }
        Ok(out)
    }

    /// Apaga temporários de importações interrompidas (queda de energia,
    /// app fechado no meio). Eles são cifrados, mas ocupam espaço.
    fn cleanup_partial_imports(&self) {
        let Ok(entries) = fs::read_dir(self.dir.join(STORES_DIR)) else { return };
        for entry in entries.flatten() {
            let name = entry.file_name();
            if name.to_string_lossy().contains(".tmp-") {
                let _ = fs::remove_file(entry.path());
            }
        }
    }
}

fn check_password(password: &str) -> Result<(), Error> {
    if password.chars().count() < MIN_PASSWORD_CHARS || password.len() > MAX_PASSWORD_BYTES {
        return Err(Error::WeakPassword);
    }
    Ok(())
}

pub(crate) fn random<const N: usize>() -> Result<[u8; N], Error> {
    let mut buf = [0u8; N];
    getrandom::fill(&mut buf).map_err(|_| Error::Random)?;
    Ok(buf)
}
