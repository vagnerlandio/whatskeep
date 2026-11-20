//! Download of WhatsApp backups stored in Google Drive.
//!
//! **Unofficial.** Uses the same path as the WhatsApp app on Android
//! (Google Play Services + `backup.googleapis.com`), like the whapa and wabdd
//! tools. Google may change or block this flow without notice; local import
//! remains the fallback.
//!
//! Security:
//! * Cookie, master token and access token live in `Zeroizing` and are never
//!   written to disk nor shown in `Debug`/errors.
//! * The master token only exists while it is exchanged for the access
//!   tokens; the caller must discard it right after (see [`login`]).
//! * The access token has the `drive.appdata` scope (only the app's hidden
//!   folder) and is valid for 1 hour.
//! * HTTP redirects are not followed (the token never goes to another host).

pub mod trace;
mod api;
pub mod auth;
mod http;

pub use api::{Api, Backup, BackupFile, find_msgstore};
pub use auth::{AccessToken, MasterToken, OauthCookie};

pub(crate) const USER_AGENT: &str = "GoogleAuth/1.4";

/// Login page whose `oauth_token` cookie is exchanged for the master token.
pub const LOGIN_URL: &str = "https://accounts.google.com/EmbeddedSetup";
pub const LOGIN_COOKIE: &str = "oauth_token";

#[derive(Clone, Debug)]
pub struct Endpoints {
    pub auth: String,
    pub backup: String,
}

impl Default for Endpoints {
    fn default() -> Self {
        Self {
            auth: "https://android.clients.google.com/auth".into(),
            backup: "https://backup.googleapis.com/v1".into(),
        }
    }
}

/// WhatsApp app on whose behalf the Drive token is requested.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DriveApp {
    WhatsApp,
    Business,
}

use serde::Serialize;

impl DriveApp {
    pub const ALL: [DriveApp; 2] = [DriveApp::WhatsApp, DriveApp::Business];

    pub fn package(self) -> &'static str {
        match self {
            DriveApp::WhatsApp => "com.whatsapp",
            DriveApp::Business => "com.whatsapp.w4b",
        }
    }

    /// SHA-1 of the app's signing certificate. WhatsApp Business is signed
    /// with the same key (to be confirmed with a real account: see README).
    pub fn client_sig(self) -> &'static str {
        "38a0f7d505fe18fec64fbf343ecaaaf310dbd799"
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("o Google recusou o login (o código do login expira em poucos minutos: tente entrar de novo)")]
    BadAuthentication,
    #[error("o Google pediu verificação no navegador; entre de novo pela janela de login")]
    NeedsBrowser,
    #[error("o Google recusou o pedido: {0}")]
    Google(String),
    #[error("a sessão do Google expirou; entre de novo")]
    TokenExpired,
    #[error("acesso negado pelo Google Drive")]
    Forbidden,
    #[error("não encontrado no Google Drive")]
    NotFound,
    #[error("o Google Drive respondeu com erro HTTP {0}")]
    Http(u16),
    #[error("falha de rede: {0}")]
    Network(String),
    #[error("falha ao configurar a conexão segura: {0}")]
    Tls(String),
    #[error("resposta inesperada do Google: {0}")]
    Unexpected(&'static str),
    #[error("o arquivo tem {0} bytes, acima do limite para baixar")]
    TooLarge(u64),
    #[error("não foi possível gravar o arquivo baixado: {0}")]
    Write(String),
    #[error("o download veio incompleto")]
    Incomplete,
    #[error("o arquivo baixado não confere com o MD5 informado pelo Google")]
    Checksum,
    #[error("cancelado")]
    Cancelled,
}

impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Self {
        // `without_url`: the URL may contain the backup path (with the phone number).
        Error::Network(e.without_url().to_string())
    }
}

/// Access tokens obtained in a login. The master token has already been discarded.
pub struct Session {
    pub tokens: Vec<(DriveApp, AccessToken)>,
    /// Apps whose token was refused (for example, no WhatsApp Business).
    pub refused: Vec<(DriveApp, String)>,
}

/// Login cookie → master token → access tokens for each app.
/// The master token is discarded (and zeroed) before returning.
pub fn login(ep: &Endpoints, email: &str, cookie: OauthCookie) -> Result<Session, Error> {
    let android_id = auth::random_android_id()?;
    let master = auth::exchange_token(ep, email, &cookie, &android_id)?;
    drop(cookie);
    let mut session = Session { tokens: Vec::new(), refused: Vec::new() };
    for app in DriveApp::ALL {
        match auth::access_token(ep, &master, &android_id, app) {
            Ok(t) => session.tokens.push((app, t)),
            Err(e) => {
                tracing::warn!(app = app.package(), "access token refused: {e}");
                session.refused.push((app, e.to_string()))
            }
        }
    }
    drop(master);
    if session.tokens.is_empty() {
        return Err(Error::Google(
            session.refused.first().map(|(_, e)| e.clone()).unwrap_or_default(),
        ));
    }
    Ok(session)
}
