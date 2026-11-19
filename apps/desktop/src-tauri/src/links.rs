//! Links in messages: they only open in the system browser, after the user
//! confirms in the UI, and only if they are http/https.

use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;
use url::Url;

use crate::error::CmdError;

/// Accepts only `http`/`https` with a host, without embedded user/password
/// (`https://bank.com@evil.com` fools the reader).
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
    let url = validate(&url).inspect_err(|_| tracing::info!("link refused (invalid)"))?;
    // The address came from a message: it does not go to the log.
    tracing::info!(scheme = url.scheme(), "external link opened in the browser");
    app.opener()
        .open_url(url.as_str(), None::<&str>)
        .map_err(|e| CmdError::new("internal", format!("não foi possível abrir o link: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_http_and_https() {
        assert!(validate("https://example.com.br/a?b=1").is_ok());
        assert!(validate("  http://example.com ").is_ok());
        for bad in [
            "javascript:alert(1)",
            "file:///etc/passwd",
            "data:text/html,<script>",
            "ftp://x.com",
            "https://bank.com@evil.com/",
            "https://user:password@x.com/",
            "tauri://localhost",
            "wamedia://localhost/media/whatsapp/1",
            "example.com",
            "",
        ] {
            assert!(validate(bad).is_err(), "{bad}");
        }
    }
}
