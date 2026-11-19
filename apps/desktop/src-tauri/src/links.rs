//! Links das mensagens: só abrem no navegador do sistema, depois que o
//! usuário confirma na UI, e só se forem http/https.

use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;
use url::Url;

use crate::error::CmdError;

/// Aceita só `http`/`https` com host, sem usuário/senha embutidos
/// (`https://banco.com@evil.com` engana quem lê).
pub fn validate(raw: &str) -> Result<Url, CmdError> {
    let bad = || CmdError::new("invalid_argument", "link inválido");
    let url = Url::parse(raw.trim()).map_err(|_| bad())?;
    let ok = matches!(url.scheme(), "http" | "https")
        && url.host_str().is_some_and(|h| !h.is_empty())
        && url.username().is_empty()
        && url.password().is_none();
    if ok { Ok(url) } else { Err(bad()) }
}

#[tauri::command]
pub fn open_link(app: AppHandle, url: String) -> Result<(), CmdError> {
    let url = validate(&url)?;
    app.opener()
        .open_url(url.as_str(), None::<&str>)
        .map_err(|e| CmdError::new("internal", format!("não foi possível abrir o link: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn so_http_e_https() {
        assert!(validate("https://exemplo.com.br/a?b=1").is_ok());
        assert!(validate("  http://exemplo.com ").is_ok());
        for bad in [
            "javascript:alert(1)",
            "file:///etc/passwd",
            "data:text/html,<script>",
            "ftp://x.com",
            "https://banco.com@evil.com/",
            "https://user:senha@x.com/",
            "tauri://localhost",
            "wamedia://localhost/media/whatsapp/1",
            "exemplo.com",
            "",
        ] {
            assert!(validate(bad).is_err(), "{bad}");
        }
    }
}
