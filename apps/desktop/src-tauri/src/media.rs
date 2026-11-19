//! Protocolo `wamedia://` para a UI carregar miniaturas (e, numa próxima
//! fase, mídias) direto do cofre em `<img src>`, sem base64 no IPC.
//!
//! `wamedia://localhost/thumb/<app>/<message_id>` (no Windows,
//! `http://wamedia.localhost/...`).
//!
//! * Só responde à janela principal e com o cofre destravado.
//! * `Cache-Control: no-store`: o webview não guarda as imagens decifradas
//!   no cache em disco.

use tauri::http::{Response, StatusCode, Uri, header};
use tauri::{AppHandle, Manager};
use wa_vault::query;

use crate::commands::source_app;
use crate::state::AppState;

pub const SCHEME: &str = "wamedia";

pub fn handle(app: &AppHandle, from_main_window: bool, uri: &Uri) -> Response<Vec<u8>> {
    if !from_main_window {
        return status(StatusCode::FORBIDDEN);
    }
    let path = percent_decode(uri.path());
    let parts: Vec<&str> = path.trim_matches('/').split('/').collect();
    let ["thumb", store, id] = parts.as_slice() else { return status(StatusCode::NOT_FOUND) };
    let (Ok(store), Ok(id)) = (source_app(store), id.parse::<i64>()) else {
        return status(StatusCode::NOT_FOUND);
    };

    match app.state::<AppState>().with_store(store, |c| Ok(query::thumbnail(c, id)?)) {
        Ok(Some(bytes)) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, sniff(&bytes))
            .header(header::CACHE_CONTROL, "no-store")
            .header(header::X_CONTENT_TYPE_OPTIONS, "nosniff")
            .body(bytes)
            .unwrap_or_else(|_| status(StatusCode::INTERNAL_SERVER_ERROR)),
        Ok(None) => status(StatusCode::NOT_FOUND),
        Err(e) if e.code == "locked" => status(StatusCode::FORBIDDEN),
        Err(_) => status(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

fn status(code: StatusCode) -> Response<Vec<u8>> {
    let mut r = Response::new(Vec::new());
    *r.status_mut() = code;
    r.headers_mut().insert(header::CACHE_CONTROL, header::HeaderValue::from_static("no-store"));
    r
}

/// Só tipos de imagem conhecidos. Qualquer outra coisa vai como binário
/// genérico (com `nosniff`, o webview não tenta interpretar).
fn sniff(bytes: &[u8]) -> &'static str {
    match bytes {
        [0xFF, 0xD8, 0xFF, ..] => "image/jpeg",
        [0x89, b'P', b'N', b'G', ..] => "image/png",
        [b'R', b'I', b'F', b'F', _, _, _, _, b'W', b'E', b'B', b'P', ..] => "image/webp",
        _ => "application/octet-stream",
    }
}

/// O `convertFileSrc` do Tauri codifica o caminho inteiro (`/` vira `%2F`).
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = bytes.get(i + 1..i + 3).and_then(|h| std::str::from_utf8(h).ok());
        if bytes[i] == b'%'
            && let Some(b) = hex.and_then(|h| u8::from_str_radix(h, 16).ok())
        {
            out.push(b);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodifica_caminho() {
        assert_eq!(percent_decode("/thumb%2Fwhatsapp%2F12"), "/thumb/whatsapp/12");
        assert_eq!(percent_decode("/a%2"), "/a%2");
        assert_eq!(percent_decode("%zz"), "%zz");
    }

    #[test]
    fn tipo_da_imagem() {
        assert_eq!(sniff(&[0xFF, 0xD8, 0xFF, 0xE0]), "image/jpeg");
        assert_eq!(sniff(b"<svg onload=x>"), "application/octet-stream");
    }
}
