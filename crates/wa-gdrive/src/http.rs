//! HTTP client with the TLS setup Google accepts on the authentication endpoint.
//!
//! `android.clients.google.com/auth` answers 403 "BadAuthentication" if the
//! client offers ALPN or does not accept session tickets (see gpsoauth,
//! `create_ssl_context`). That is why rustls is assembled by hand: no ALPN
//! (and therefore HTTP/1.1 only), tickets enabled (the rustls default) and
//! certificates verified by the operating system.

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
        // reqwest does not change the ALPN of a preconfigured config.
        .tls_backend_preconfigured(tls_config()?)
        .connect_timeout(Duration::from_secs(20))
        // Never redirect the token to another host.
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(crate::USER_AGENT)
        .timeout(timeout)
        .build()
        .map_err(Error::from)
}

#[cfg(test)]
mod tests {
    #[test]
    fn tls_without_alpn_and_with_tickets() {
        let tls = super::tls_config().unwrap();
        assert!(tls.alpn_protocols.is_empty());
        // No public getter: Debug shows the policy (the rustls default).
        assert!(format!("{:?}", tls.resumption).contains("SessionIdOrTickets"));
    }

    /// Real handshake with Google (no account data: empty form).
    /// `cargo test -p wa-gdrive -- --ignored`
    #[test]
    #[ignore]
    fn real_handshake_with_google() {
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
