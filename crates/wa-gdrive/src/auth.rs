//! Autenticação do Google Play Services (o que o Android faz ao adicionar
//! uma conta), reimplementada a partir do gpsoauth.
//!
//! ```text
//! cookie oauth_token (login no EmbeddedSetup)
//!   └─exchange_token──► master token (aas_et/…)   acesso amplo à conta: só em memória
//!         └─access_token(app, client_sig)──► token OAuth de 1 h, escopo drive.appdata
//! ```

use std::fmt;

use url::form_urlencoded;
use zeroize::Zeroizing;

use crate::{DriveApp, Error, Endpoints, http};

/// Assinatura do Google Play Services (usada na troca do cookie).
const GMS_CLIENT_SIG: &str = "38918a453d07199354f8b19af05ec6562ced5788";
const DRIVE_SCOPE: &str = "oauth2:https://www.googleapis.com/auth/drive.appdata";
const GMS_VERSION: &str = "240913000";

macro_rules! secret_string {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        pub struct $name(Zeroizing<String>);

        impl $name {
            pub fn new(s: String) -> Self {
                Self(Zeroizing::new(s))
            }
            pub fn expose(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(concat!(stringify!($name), "(<redacted>)"))
            }
        }
    };
}

secret_string!(OauthCookie, "Cookie `oauth_token` do login no EmbeddedSetup (uso único, curta duração).");
secret_string!(MasterToken, "Token mestre da conta (`aas_et/…`). Equivale a acesso amplo à conta Google.");
secret_string!(AccessToken, "Token OAuth de 1 hora, só com o escopo `drive.appdata` do app.");

/// Id de dispositivo Android aleatório (16 hex), novo a cada login.
pub fn random_android_id() -> Result<String, Error> {
    let mut b = [0u8; 8];
    getrandom(&mut b)?;
    Ok(b.iter().map(|x| format!("{x:02x}")).collect())
}

fn getrandom(buf: &mut [u8]) -> Result<(), Error> {
    rustls::crypto::ring::default_provider()
        .secure_random
        .fill(buf)
        .map_err(|_| Error::Tls("sem fonte de números aleatórios".into()))
}

/// Troca o cookie do login pelo token mestre.
pub fn exchange_token(
    ep: &Endpoints,
    email: &str,
    cookie: &OauthCookie,
    android_id: &str,
) -> Result<MasterToken, Error> {
    let body = Zeroizing::new(
        form_urlencoded::Serializer::new(String::new())
            .append_pair("accountType", "HOSTED_OR_GOOGLE")
            .append_pair("Email", email)
            .append_pair("has_permission", "1")
            .append_pair("add_account", "1")
            .append_pair("ACCESS_TOKEN", "1")
            .append_pair("Token", cookie.expose())
            .append_pair("service", "ac2dm")
            .append_pair("source", "android")
            .append_pair("androidId", android_id)
            .append_pair("device_country", "us")
            .append_pair("operatorCountry", "us")
            .append_pair("lang", "en")
            .append_pair("sdk_version", "17")
            .append_pair("google_play_services_version", GMS_VERSION)
            .append_pair("client_sig", GMS_CLIENT_SIG)
            .append_pair("callerSig", GMS_CLIENT_SIG)
            .append_pair("droidguard_results", "dummy123")
            .finish(),
    );
    let mut resp = post_auth(ep, &body)?;
    let token = take(&mut resp, "Token").ok_or_else(|| auth_error(&resp))?;
    Ok(MasterToken::new(token))
}

/// Pede um token de acesso ao Drive em nome do app do WhatsApp.
pub fn access_token(
    ep: &Endpoints,
    master: &MasterToken,
    android_id: &str,
    app: DriveApp,
) -> Result<AccessToken, Error> {
    let body = Zeroizing::new(
        form_urlencoded::Serializer::new(String::new())
            .append_pair("accountType", "HOSTED_OR_GOOGLE")
            .append_pair("has_permission", "1")
            .append_pair("Token", master.expose())
            .append_pair("service", DRIVE_SCOPE)
            .append_pair("source", "android")
            .append_pair("androidId", android_id)
            .append_pair("app", app.package())
            .append_pair("client_sig", app.client_sig())
            .append_pair("device_country", "us")
            .append_pair("operatorCountry", "us")
            .append_pair("lang", "en")
            .append_pair("sdk_version", "17")
            .append_pair("google_play_services_version", GMS_VERSION)
            .finish(),
    );
    let mut resp = post_auth(ep, &body)?;
    let token = take(&mut resp, "Auth").ok_or_else(|| auth_error(&resp))?;
    Ok(AccessToken::new(token))
}

/// Resposta `chave=valor` por linha. Os valores podem conter tokens, então o
/// vetor inteiro é zerado no drop.
struct AuthResponse(Vec<(String, Zeroizing<String>)>);

fn post_auth(ep: &Endpoints, body: &str) -> Result<AuthResponse, Error> {
    let started = std::time::Instant::now();
    tracing::debug!(
        target: "wa_gdrive::http",
        "→ POST {} {}",
        ep.auth,
        crate::trace::form(form_urlencoded::parse(body.as_bytes()).collect::<Vec<_>>().iter().map(|(k, v)| (k.as_ref(), v.as_ref())))
    );
    let result = http::client(Some(std::time::Duration::from_secs(30)))?
        .post(&ep.auth)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .header("Accept-Encoding", "identity")
        .body(body.to_owned())
        .send();
    let resp = match result {
        Ok(r) => r,
        Err(e) => {
            let err: Error = e.into();
            tracing::warn!(target: "wa_gdrive::http", ms = started.elapsed().as_millis() as u64, "✗ POST {} falhou: {}", ep.auth, err);
            return Err(err);
        }
    };
    let status = resp.status().as_u16();
    let text = Zeroizing::new(resp.text()?);
    let parsed = parse_auth_response(&text);
    let shown = crate::trace::form(parsed.0.iter().map(|(k, v)| (k.as_str(), v.as_str())));
    if status == 200 {
        tracing::debug!(target: "wa_gdrive::http", status, ms = started.elapsed().as_millis() as u64, "← {shown}");
    } else {
        tracing::warn!(target: "wa_gdrive::http", status, ms = started.elapsed().as_millis() as u64, "← {shown}");
    }
    Ok(parsed)
}

fn parse_auth_response(text: &str) -> AuthResponse {
    AuthResponse(
        text.lines()
            .filter_map(|l| l.split_once('='))
            .map(|(k, v)| (k.to_owned(), Zeroizing::new(v.to_owned())))
            .collect(),
    )
}

/// Remove a chave da resposta. O valor original (zerável) é zerado no drop;
/// a cópia devolvida vai direto para um tipo que também zera.
fn take(resp: &mut AuthResponse, key: &str) -> Option<String> {
    let i = resp.0.iter().position(|(k, _)| k == key)?;
    let (_, v) = resp.0.swap_remove(i);
    Some(v.as_str().to_owned())
}

/// Converte o erro do Google numa mensagem útil. Só repassa campos
/// conhecidos e sem segredo (`Error`, `ErrorDetail`, `Url` não entra).
fn auth_error(resp: &AuthResponse) -> Error {
    let get = |k: &str| resp.0.iter().find(|(key, _)| key == k).map(|(_, v)| v.as_str().to_owned());
    match get("Error").as_deref() {
        Some("BadAuthentication") => Error::BadAuthentication,
        Some("NeedsBrowser") => Error::NeedsBrowser,
        Some(other) => Error::Google(other.chars().take(80).collect()),
        None => Error::Google("resposta inesperada".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resposta_de_autenticacao() {
        let mut r = parse_auth_response("SID=x\nToken=aas_et/abc=def\nservices=mail\n");
        assert_eq!(take(&mut r, "Token").as_deref(), Some("aas_et/abc=def"));
        assert_eq!(take(&mut r, "Token"), None);
        assert!(matches!(auth_error(&parse_auth_response("Error=BadAuthentication\n")), Error::BadAuthentication));
        assert!(matches!(auth_error(&parse_auth_response("Error=NeedsBrowser\nUrl=https://x\n")), Error::NeedsBrowser));
    }

    #[test]
    fn debug_nao_vaza() {
        let t = MasterToken::new("aas_et/segredo".into());
        assert_eq!(format!("{t:?}"), "MasterToken(<redacted>)");
    }

    #[test]
    fn android_id() {
        let id = random_android_id().unwrap();
        assert_eq!(id.len(), 16);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
