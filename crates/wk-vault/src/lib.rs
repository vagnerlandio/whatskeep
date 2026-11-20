//! Encrypted vault where the imported history is kept.
//!
//! ```text
//! <dir>/
//!   vault.json                Argon2id parameters + wrapped DEK (AES-256-GCM)
//!   .metadata_never_index     Spotlight does not index it
//!   stores/whatsapp.db        SQLCipher, key = HKDF(DEK, vault id, "wk-vault store v2:whatsapp")
//!   stores/business.db        SQLCipher, key = HKDF(DEK, vault id, "wk-vault store v2:business")
//! ```
//!
//! * The app password only serves to unwrap the DEK. Without it, nothing on disk
//!   is readable, and there is no recovery.
//! * A [`Vault`] only exists unlocked: locking means dropping it, and the DEK is zeroed on drop.
//! * Folders get 0700 permissions and files 0600 (Unix).

mod fsutil;
mod header;
pub mod media;
pub mod query;
mod store;

use std::fs;
use std::path::{Path, PathBuf};

use hkdf::Hkdf;
use sha2::Sha256;
use wk_import::SourceApp;
use zeroize::Zeroizing;

pub use header::KdfParams;
pub use media::{BlobRef, MediaHandle, MediaReader, MediaWriter, Put};
pub use store::{StoreHandle, StoreInfo};

use header::Header;

/// 256-bit symmetric key, zeroed on drop.
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
    #[error("este cofre foi criado por uma versão de desenvolvimento anterior do WhatsKeep e não pode ser aberto; apague a pasta do cofre e crie um novo")]
    OutdatedFormat,
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
    Import(#[from] wk_import::Error),
}

/// Unlocked vault. Locking means `drop`ping it.
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

    /// Creates a new vault and returns it already unlocked.
    ///
    /// Refuses if the folder already has a header or databases: databases without
    /// the original header are unreadable, but deleting them is the user's decision.
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
        // The data is already encrypted; kept out of Time Machine, not even the
        // encrypted version ends up scattered across backup disks.
        let _ = fsutil::exclude_from_backup(dir);

        // Straight into the zeroizable buffer, with no intermediate copy on the stack.
        let mut dek = Key::default();
        getrandom::fill(dek.as_mut()).map_err(|_| Error::Random)?;
        let header = Header::create(password, params, &dek)?;
        header.write_atomic(&dir.join(HEADER_FILE))?;
        Ok(Self { dir: dir.to_owned(), header, dek })
    }

    /// Unlocks with the password. Costs one Argon2id (~0.5 s with the default parameters on a recent Mac).
    pub fn unlock(dir: &Path, password: &str) -> Result<Self, Error> {
        let path = dir.join(HEADER_FILE);
        if !path.is_file() {
            return Err(Error::NotFound);
        }
        let header = Header::read(&path)?;
        let dek = header.unwrap_key(password)?;
        let vault = Self { dir: dir.to_owned(), header, dek };
        vault.cleanup_partial_imports();
        // Idempotent: covers vaults created before this protection existed.
        let _ = fsutil::exclude_from_backup(dir);
        Ok(vault)
    }

    /// Changes the password. Asks for the current password again so that someone in
    /// front of an unlocked app cannot change it.
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

    /// Argon2id parameters in use.
    pub fn kdf_params(&self) -> KdfParams {
        self.header.params()
    }

    pub fn store(&self, app: SourceApp) -> Result<StoreHandle, Error> {
        let vault_id = self.header.vault_id()?;
        let mut key = Key::default();
        Hkdf::<Sha256>::new(Some(&vault_id), self.dek.as_ref())
            .expand(format!("wk-vault store v2:{}", app.as_str()).as_bytes(), key.as_mut())
            .expect("32 bytes fit in HKDF-SHA256");
        Ok(StoreHandle::new(app, &self.dir.join(STORES_DIR), key))
    }

    /// An app's media: encrypted blobs + index, with their own keys.
    pub fn media(&self, app: SourceApp) -> Result<MediaHandle, Error> {
        let derive = |label: &str| -> Result<Key, Error> {
            let mut key = Key::default();
            Hkdf::<Sha256>::new(Some(&self.header.vault_id()?), self.dek.as_ref())
                .expand(format!("wk-vault {label} v2:{}", app.as_str()).as_bytes(), key.as_mut())
                .expect("32 bytes fit in HKDF-SHA256");
            Ok(key)
        };
        Ok(MediaHandle::new(&self.dir.join(STORES_DIR), app.as_str(), derive("media")?, derive("media-index")?))
    }

    /// Deletes an app's history: its database and its media (blobs and index).
    /// The caller closes its own connections to them first.
    pub fn delete_store(&self, app: SourceApp) -> Result<(), Error> {
        self.store(app)?.delete()?;
        self.media(app)?.delete()?;
        Ok(())
    }

    /// Deletes the whole vault in `dir`. Needs no password: it is the way out
    /// for a forgotten one (the data cannot be read without it anyway). The
    /// header goes first, so nothing left in the folder can be opened even if
    /// removing the rest stops midway.
    pub fn destroy(dir: &Path) -> Result<(), Error> {
        fsutil::remove_if_exists(fs::remove_file(dir.join(HEADER_FILE)))?;
        fsutil::remove_if_exists(fs::remove_dir_all(dir))?;
        Ok(())
    }

    /// Bytes the vault in `dir` takes on disk.
    pub fn disk_usage(dir: &Path) -> u64 {
        fsutil::disk_usage(dir)
    }

    /// Databases already imported, with counts.
    pub fn stores(&self) -> Result<Vec<StoreInfo>, Error> {
        let mut out = Vec::new();
        for app in SourceApp::ALL {
            if let Some(info) = self.store(app)?.info()? {
                out.push(info);
            }
        }
        Ok(out)
    }

    /// Deletes temporary files from interrupted imports (power loss,
    /// app closed midway). They are encrypted, but take up space.
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
