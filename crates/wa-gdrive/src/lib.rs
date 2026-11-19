//! Download de backups do WhatsApp guardados no Google Drive.
//!
//! **Não oficial.** Usa o mesmo caminho do app do WhatsApp no Android
//! (Google Play Services + `backup.googleapis.com`), como as ferramentas
//! whapa e wabdd. O Google pode mudar ou bloquear esse fluxo sem aviso; a
//! importação local continua sendo o plano B.
//!
//! Segurança:
//! * Cookie, token mestre e token de acesso ficam em `Zeroizing` e nunca são
//!   gravados nem aparecem em `Debug`/erros.
//! * O token mestre só existe durante a troca pelos tokens de acesso; quem
//!   chama deve descartá-lo em seguida (veja [`login`]).
//! * O token de acesso tem escopo `drive.appdata` (só a pasta oculta do app)
//!   e vale 1 hora.
//! * Redirecionamentos HTTP não são seguidos (o token não vai para outro host).

pub mod trace;
mod api;
pub mod auth;
mod http;

pub use api::{Api, Backup, BackupFile, find_msgstore};
pub use auth::{AccessToken, MasterToken, OauthCookie};

pub(crate) const USER_AGENT: &str = "GoogleAuth/1.4";

/// Página de login cujo cookie `oauth_token` é trocado pelo token mestre.
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

/// App do WhatsApp em nome do qual o token do Drive é pedido.
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

    /// SHA-1 do certificado de assinatura do app. O WhatsApp Business é
    /// assinado pela mesma chave (a confirmar com uma conta real: ver README).
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
    #[error("o download veio incompleto")]
    Incomplete,
    #[error("o arquivo baixado não confere com o MD5 informado pelo Google")]
    Checksum,
    #[error("cancelado")]
    Cancelled,
}

impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Self {
        // `without_url`: a URL pode ter o caminho do backup (com o telefone).
        Error::Network(e.without_url().to_string())
    }
}

/// Tokens de acesso obtidos num login. O token mestre já foi descartado.
pub struct Session {
    pub tokens: Vec<(DriveApp, AccessToken)>,
    /// Apps cujo token foi recusado (por exemplo, sem WhatsApp Business).
    pub refused: Vec<(DriveApp, String)>,
}

/// Cookie do login → token mestre → tokens de acesso para cada app.
/// O token mestre é descartado (e zerado) antes de retornar.
pub fn login(ep: &Endpoints, email: &str, cookie: OauthCookie) -> Result<Session, Error> {
    let android_id = auth::random_android_id()?;
    let master = auth::exchange_token(ep, email, &cookie, &android_id)?;
    drop(cookie);
    let mut session = Session { tokens: Vec::new(), refused: Vec::new() };
    for app in DriveApp::ALL {
        match auth::access_token(ep, &master, &android_id, app) {
            Ok(t) => session.tokens.push((app, t)),
            Err(e) => {
                tracing::warn!(app = app.package(), "token de acesso recusado: {e}");
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
