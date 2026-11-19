//! `wamedia://` protocol so the UI can load thumbnails (and media) straight
//! from the vault in `<img src>`, without base64 over the IPC.
//!
//! * `wamedia://localhost/thumb/<app>/<message_id>`: thumbnail.
//! * `wamedia://localhost/media/<app>/<message_id>`: full file, with `Range`
//!   support (video/audio seeking without decrypting the whole file).
//!
//! On Windows the address is `http://wamedia.localhost/...`.
//!
//! * Only answers the main window, and only with the vault unlocked.
//! * `Cache-Control: no-store`: the webview does not keep the decrypted
//!   images in its disk cache.

use tauri::http::{Request, Response, StatusCode, header};
use tauri::{AppHandle, Manager};
use wa_vault::query;

use wa_import::SourceApp;

use crate::commands::source_app;
use crate::error::CmdError;
use crate::state::AppState;

pub const SCHEME: &str = "wamedia";

/// Largest range returned per request: videos are requested in chunks.
const MAX_CHUNK: u64 = 4 << 20;
/// Without `Range`, files up to this size are sent whole (photos).
const MAX_WHOLE: u64 = 64 << 20;

pub fn handle(app: &AppHandle, from_main_window: bool, request: &Request<Vec<u8>>) -> Response<Vec<u8>> {
    if !from_main_window {
        return status(StatusCode::FORBIDDEN);
    }
    let path = percent_decode(request.uri().path());
    let parts: Vec<&str> = path.trim_matches('/').split('/').collect();
    let [kind @ ("thumb" | "media"), store, id] = parts.as_slice() else { return status(StatusCode::NOT_FOUND) };
    let (Ok(store), Ok(id)) = (source_app(store), id.parse::<i64>()) else {
        return status(StatusCode::NOT_FOUND);
    };
    let state = app.state::<AppState>();
    let result = if *kind == "thumb" {
        thumb(&state, store, id)
    } else {
        let range = request.headers().get(header::RANGE).and_then(|v| v.to_str().ok()).map(str::to_owned);
        media(&state, store, id, range.as_deref())
    };
    match result {
        Ok(r) => {
            tracing::trace!(kind = kind, status = r.status().as_u16(), bytes = r.body().len(), "wamedia");
            r
        }
        Err(e) if e.code == "locked" => status(StatusCode::FORBIDDEN),
        Err(e) if e.code == "not_found" => {
            tracing::debug!(kind = kind, message = id, "wamedia: not found");
            status(StatusCode::NOT_FOUND)
        }
        Err(e) => {
            tracing::warn!(kind = kind, message = id, code = e.code, "wamedia: error: {}", e.message);
            status(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

fn thumb(state: &AppState, store: SourceApp, id: i64) -> Result<Response<Vec<u8>>, CmdError> {
    match state.with_store(store, |c| Ok(query::thumbnail(c, id)?))? {
        Some(bytes) => Ok(ok_response(StatusCode::OK, sniff(&bytes), bytes)),
        None => Err(CmdError::new("not_found", "")),
    }
}

fn media(state: &AppState, store: SourceApp, id: i64, range: Option<&str>) -> Result<Response<Vec<u8>>, CmdError> {
    let r = state
        .with_store(store, |c| Ok(query::media_ref(c, id)?))?
        .ok_or_else(|| CmdError::new("not_found", ""))?;
    state.with_media(store, |m| {
        let m = m.ok_or_else(|| CmdError::new("not_found", ""))?;
        let blob = m
            .find(r.file_hash.as_deref(), r.file_path.as_deref())?
            .ok_or_else(|| CmdError::new("not_found", ""))?;
        let ctype = safe_mime(blob.mime.as_deref().or(r.mime.as_deref()));
        let size = blob.size;

        let (start, end, partial) = match range.and_then(|h| parse_range(h, size)) {
            Some((s, e)) => (s, e.min(s + MAX_CHUNK), true),
            None if size <= MAX_WHOLE => (0, size, false),
            None => (0, MAX_CHUNK.min(size), true),
        };
        if range.is_some() && start >= size && size > 0 {
            let mut resp = status(StatusCode::RANGE_NOT_SATISFIABLE);
            resp.headers_mut()
                .insert(header::CONTENT_RANGE, header::HeaderValue::from_str(&format!("bytes */{size}")).unwrap());
            return Ok(resp);
        }
        // The zeroizable buffer is copied into the response body (Tauri requires a Vec).
        let body = m.read_range(&blob, start, end)?.to_vec();
        let mut resp = ok_response(if partial { StatusCode::PARTIAL_CONTENT } else { StatusCode::OK }, ctype, body);
        let h = resp.headers_mut();
        h.insert(header::ACCEPT_RANGES, header::HeaderValue::from_static("bytes"));
        if partial {
            let v = format!("bytes {start}-{}/{size}", end.saturating_sub(1));
            h.insert(header::CONTENT_RANGE, header::HeaderValue::from_str(&v).unwrap());
        }
        Ok(resp)
    })
}

fn ok_response(code: StatusCode, ctype: &str, body: Vec<u8>) -> Response<Vec<u8>> {
    Response::builder()
        .status(code)
        .header(header::CONTENT_TYPE, ctype)
        .header(header::CACHE_CONTROL, "no-store")
        .header(header::X_CONTENT_TYPE_OPTIONS, "nosniff")
        .body(body)
        .unwrap_or_else(|_| status(StatusCode::INTERNAL_SERVER_ERROR))
}

/// `bytes=a-b`, `bytes=a-` or `bytes=-n` → `[start, end)`. A single range only.
fn parse_range(h: &str, size: u64) -> Option<(u64, u64)> {
    let spec = h.trim().strip_prefix("bytes=")?;
    if spec.contains(',') {
        return None;
    }
    let (a, b) = spec.split_once('-')?;
    match (a.trim(), b.trim()) {
        ("", n) => {
            let n: u64 = n.parse().ok()?;
            Some((size.saturating_sub(n), size))
        }
        (a, "") => Some((a.parse().ok()?, size)),
        (a, b) => {
            let (a, b): (u64, u64) = (a.parse().ok()?, b.parse().ok()?);
            (b >= a).then_some((a, (b + 1).min(size)))
        }
    }
}

/// Only types the webview displays as media. Anything else (HTML, SVG,
/// PDF...) goes out as generic binary: a received file never becomes a page.
fn safe_mime(mime: Option<&str>) -> &'static str {
    let m = mime.unwrap_or("").split(';').next().unwrap_or("").trim().to_ascii_lowercase();
    match m.as_str() {
        "image/jpeg" => "image/jpeg",
        "image/png" => "image/png",
        "image/webp" => "image/webp",
        "image/gif" => "image/gif",
        "video/mp4" => "video/mp4",
        "video/3gpp" => "video/3gpp",
        "video/quicktime" => "video/quicktime",
        "audio/ogg" | "audio/opus" => "audio/ogg",
        "audio/mpeg" => "audio/mpeg",
        "audio/mp4" | "audio/aac" | "audio/m4a" => "audio/mp4",
        "audio/amr" => "audio/amr",
        _ => "application/octet-stream",
    }
}

fn status(code: StatusCode) -> Response<Vec<u8>> {
    let mut r = Response::new(Vec::new());
    *r.status_mut() = code;
    r.headers_mut().insert(header::CACHE_CONTROL, header::HeaderValue::from_static("no-store"));
    r
}

/// Only known image types. Anything else goes out as generic binary (with
/// `nosniff`, the webview does not try to interpret it).
fn sniff(bytes: &[u8]) -> &'static str {
    match bytes {
        [0xFF, 0xD8, 0xFF, ..] => "image/jpeg",
        [0x89, b'P', b'N', b'G', ..] => "image/png",
        [b'R', b'I', b'F', b'F', _, _, _, _, b'W', b'E', b'B', b'P', ..] => "image/webp",
        _ => "application/octet-stream",
    }
}

/// Tauri's `convertFileSrc` encodes the whole path (`/` becomes `%2F`).
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
    fn decodes_path() {
        assert_eq!(percent_decode("/thumb%2Fwhatsapp%2F12"), "/thumb/whatsapp/12");
        assert_eq!(percent_decode("/a%2"), "/a%2");
        assert_eq!(percent_decode("%zz"), "%zz");
    }

    #[test]
    fn ranges() {
        assert_eq!(parse_range("bytes=0-99", 1000), Some((0, 100)));
        assert_eq!(parse_range("bytes=900-", 1000), Some((900, 1000)));
        assert_eq!(parse_range("bytes=-100", 1000), Some((900, 1000)));
        assert_eq!(parse_range("bytes=990-2000", 1000), Some((990, 1000)));
        assert_eq!(parse_range("bytes=5-1", 1000), None);
        assert_eq!(parse_range("bytes=0-1,5-9", 1000), None);
        assert_eq!(parse_range("items=0-1", 1000), None);
    }

    #[test]
    fn safe_types() {
        assert_eq!(safe_mime(Some("audio/ogg; codecs=opus")), "audio/ogg");
        assert_eq!(safe_mime(Some("text/html")), "application/octet-stream");
        assert_eq!(safe_mime(Some("image/svg+xml")), "application/octet-stream");
        assert_eq!(safe_mime(None), "application/octet-stream");
    }

    #[test]
    fn image_type() {
        assert_eq!(sniff(&[0xFF, 0xD8, 0xFF, 0xE0]), "image/jpeg");
        assert_eq!(sniff(b"<svg onload=x>"), "application/octet-stream");
    }
}
