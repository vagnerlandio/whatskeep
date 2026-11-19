//! Cliente HTTP com o TLS que o Google aceita no endpoint de autenticação.
//!
//! O `android.clients.google.com/auth` responde 403 "BadAuthentication" se o
//! cliente oferecer ALPN ou não aceitar session tickets (ver gpsoauth,
//! `create_ssl_context`). Por isso o rustls é montado à mão: sem ALPN (e
//! portanto só HTTP/1.1), tickets ligados (padrão do rustls) e certificados
//! verificados pelo sistema operacional.

use std::sync::Arc;
use std::time::Duration;

use reqwest::blocking::Client;

use crate::Error;

pub(crate) fn tls_config() -> Result<rustls::ClientConfig, Error> {
    use rustls_platform_verifier::BuilderVerifierExt;

    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut tls = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| Error::Tls(e.to_string()))?
        .with_platform_verifier()
        .map_err(|e| Error::Tls(e.to_string()))?
        .with_no_client_auth();
    tls.alpn_protocols.clear();
    Ok(tls)
}

pub(crate) fn client(timeout: Option<Duration>) -> Result<Client, Error> {
    Client::builder()
        // O reqwest não altera o ALPN de uma configuração pré-montada.
        .tls_backend_preconfigured(tls_config()?)
        .connect_timeout(Duration::from_secs(20))
        // Nada de redirecionar o token para outro host.
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(crate::USER_AGENT)
        .timeout(timeout)
        .build()
        .map_err(Error::from)
}

#[cfg(test)]
mod tests {
    #[test]
    fn tls_sem_alpn_e_com_tickets() {
        let tls = super::tls_config().unwrap();
        assert!(tls.alpn_protocols.is_empty());
        // Sem getter público: o Debug mostra a política (padrão do rustls).
        assert!(format!("{:?}", tls.resumption).contains("SessionIdOrTickets"));
    }

    /// Handshake real com o Google (sem dados de conta: formulário vazio).
    /// `cargo test -p wa-gdrive -- --ignored`
    #[test]
    #[ignore]
    fn handshake_real_com_o_google() {
        let resp = super::client(Some(std::time::Duration::from_secs(20)))
            .unwrap()
            .post(crate::Endpoints::default().auth)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body("")
            .send()
            .unwrap();
        let status = resp.status();
        let text = resp.text().unwrap();
        println!("HTTP {status}: {}", text.lines().next().unwrap_or_default());
        assert!(text.contains("Error="), "{text}");
    }
}
