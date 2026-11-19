//! Single error type for the commands: `{ code, message }` for the frontend.
//!
//! The `code` is stable (the UI decides what to do with it). The `message` is
//! for humans and never contains key material or message content.

use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct CmdError {
    pub code: &'static str,
    pub message: String,
}

impl CmdError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self { code, message: message.into() }
    }

    pub fn locked() -> Self {
        Self::new("locked", "o cofre está travado")
    }
}

impl From<wa_vault::Error> for CmdError {
    fn from(e: wa_vault::Error) -> Self {
        use wa_vault::Error as E;
        let code = match &e {
            E::WeakPassword => "weak_password",
            E::WrongPassword => "wrong_password",
            E::AlreadyExists => "already_exists",
            E::NotFound => "not_found",
            E::Unsupported | E::Corrupt(_) | E::StoreUnreadable => "vault_corrupt",
            E::Import(inner) => return Self::from_import(inner),
            E::Io(inner) => return Self::from(inner),
            E::Random | E::Sqlite(_) => "internal",
        };
        Self::new(code, e.to_string())
    }
}

impl CmdError {
    fn from_import(e: &wa_import::Error) -> Self {
        let code = match e {
            wa_import::Error::NotMsgstore => "not_msgstore",
            wa_import::Error::DestinationNotEmpty | wa_import::Error::Sqlite(_) => "internal",
        };
        Self::new(code, e.to_string())
    }
}

impl From<wa_import::Error> for CmdError {
    fn from(e: wa_import::Error) -> Self {
        Self::from_import(&e)
    }
}

impl From<wa_crypt::Error> for CmdError {
    fn from(e: wa_crypt::Error) -> Self {
        use wa_crypt::Error as E;
        let code = match &e {
            E::InvalidKey(_) | E::UnknownKeyFile => "invalid_key",
            E::KeyMismatch { .. } => "key_mismatch",
            E::Authentication => "wrong_key",
            E::Header(_) | E::Unsupported | E::Truncated => "invalid_backup",
            E::Decompress(_) | E::TooLarge(_) => "invalid_backup",
        };
        Self::new(code, e.to_string())
    }
}

impl From<&std::io::Error> for CmdError {
    fn from(e: &std::io::Error) -> Self {
        use std::io::ErrorKind as K;
        let message = match e.kind() {
            K::NotFound => "arquivo não encontrado".to_owned(),
            K::PermissionDenied => "sem permissão para acessar o arquivo".to_owned(),
            K::OutOfMemory => "memória insuficiente".to_owned(),
            _ => format!("erro de arquivo: {e}"),
        };
        Self::new("io", message)
    }
}

impl From<std::io::Error> for CmdError {
    fn from(e: std::io::Error) -> Self {
        Self::from(&e)
    }
}

impl From<rusqlite::Error> for CmdError {
    fn from(e: rusqlite::Error) -> Self {
        Self::from(wa_vault::Error::from(e))
    }
}
