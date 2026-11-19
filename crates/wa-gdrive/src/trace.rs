//! Request/response logging for diagnostics (target `wa_gdrive::http`).
//!
//! Everything is redacted **before** it reaches the log:
//! * tokens, cookie, e-mail, device id and `Authorization` never appear
//!   (they become `<redacted:N>`);
//! * the backup id in the path (`clients/wa/backups/<phone>`) becomes `<backup>`;
//! * media paths become `<folder>/…<.ext>`; only `Databases/...` is shown;
//! * in JSON, only numeric/boolean values and fields from a safe list are
//!   shown; other strings become `<text:N>`;
//! * long digit runs (phone numbers) in error messages become `<n>`.

use serde_json::Value;

/// Form fields (login) whose value may be shown.
const SAFE_FORM_KEYS: &[&str] = &[
    "accountType",
    "has_permission",
    "add_account",
    "ACCESS_TOKEN",
    "service",
    "source",
    "device_country",
    "operatorCountry",
    "lang",
    "sdk_version",
    "google_play_services_version",
    "client_sig",
    "callerSig",
    "app",
    // Responses from the authentication endpoint.
    "Error",
    "ErrorDetail",
    "Expiry",
    "issueAdvice",
    "isTokenSnowballed",
    "storeConsentRemotely",
    "grantedScopes",
];

/// JSON fields (backup API) whose strings may be shown.
const SAFE_JSON_KEYS: &[&str] = &[
    "updateTime",
    "sizeBytes",
    "versionOfAppWhenBackup",
    "backupFrequency",
    "backupNetworkSettings",
    "status",
    "code",
    "reason",
    "domain",
    "@type",
];

fn redacted(v: &str) -> String {
    format!("<redacted:{}>", v.chars().count())
}

/// `key=value&...` or `key=value\n...` with sensitive values redacted.
pub fn form<'a>(pairs: impl IntoIterator<Item = (&'a str, &'a str)>) -> String {
    pairs
        .into_iter()
        .map(|(k, v)| {
            if matches!(k, "Error" | "ErrorDetail") {
                format!("{k}={}", scrub_digits(v))
            } else if SAFE_FORM_KEYS.contains(&k) {
                // Protocol constants (versions, app signature, scopes).
                format!("{k}={v}")
            } else {
                format!("{k}={}", redacted(v))
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// API path without the backup id or media names.
pub fn path(p: &str) -> String {
    let mut out = Vec::new();
    let mut parts = p.split('/').filter(|s| !s.is_empty()).peekable();
    while let Some(seg) = parts.next() {
        out.push(seg.to_owned());
        if seg == "backups" && parts.peek().is_some() {
            parts.next();
            out.push("<backup>".into());
        } else if seg == "files" && parts.peek().is_some() {
            let rest: Vec<&str> = parts.by_ref().collect();
            out.push(file_path(&rest.join("/")));
        }
    }
    out.join("/")
}

/// `Databases/msgstore.db.crypt15` is kept; the rest becomes `<folder>/…<.ext>`.
pub fn file_path(p: &str) -> String {
    if p.starts_with("Databases/") && !p.contains("..") {
        return p.to_owned();
    }
    let top = p.split_once('/').map(|(t, _)| t).unwrap_or("");
    let ext = p.rsplit_once('.').map(|(_, e)| e).filter(|e| e.len() <= 8 && !e.contains('/')).unwrap_or("");
    let ext = if ext.is_empty() { String::new() } else { format!(".{ext}") };
    if top.is_empty() { format!("…{ext}") } else { format!("{top}/…{ext}") }
}

/// Replaces runs of 8+ digits (phone numbers, ids) with `<n>`.
pub fn scrub_digits(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut run = String::new();
    for c in s.chars() {
        if c.is_ascii_digit() {
            run.push(c);
        } else {
            flush_digits(&mut out, &mut run);
            out.push(c);
        }
    }
    flush_digits(&mut out, &mut run);
    out
}

fn flush_digits(out: &mut String, run: &mut String) {
    if run.len() >= 8 {
        out.push_str("<n>");
    } else {
        out.push_str(run);
    }
    run.clear();
}

fn is_count_key(k: &str) -> bool {
    let l = k.to_ascii_lowercase();
    l.ends_with("size") || l.ends_with("sizebytes") || l.starts_with("numof") || l.ends_with("count")
}

/// JSON with the same structure, but only with the safe values. Long lists
/// show the first 2 items and the total. The `metadata` field (JSON inside a
/// string) is parsed and redacted the same way.
pub fn json(v: &Value) -> Value {
    shape(v, None)
}

fn shape(v: &Value, key: Option<&str>) -> Value {
    match v {
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| {
                    let shown = match (k.as_str(), v) {
                        ("name", Value::String(s)) => Value::String(path(s)),
                        ("metadata", Value::String(s)) => match serde_json::from_str::<Value>(s) {
                            Ok(inner) => shape(&inner, Some(k)),
                            Err(_) => Value::String(format!("<text:{}>", s.len())),
                        },
                        ("message", Value::String(s)) => Value::String(scrub_digits(s)),
                        _ => shape(v, Some(k)),
                    };
                    (k.clone(), shown)
                })
                .collect(),
        ),
        Value::Array(items) => {
            let mut out: Vec<Value> = items.iter().take(2).map(|i| shape(i, key)).collect();
            if items.len() > 2 {
                out.push(Value::String(format!("… +{} items (total {})", items.len() - 2, items.len())));
            }
            Value::Array(out)
        }
        Value::String(s) if key.is_some_and(|k| SAFE_JSON_KEYS.contains(&k)) => Value::String(scrub_digits(s)),
        // Sizes and counts come as strings ("chatdbSize": "1234").
        Value::String(s) if key.is_some_and(is_count_key) && !s.is_empty() && s.len() <= 15 && s.chars().all(|c| c.is_ascii_digit()) => {
            Value::String(s.clone())
        }
        Value::String(s) => Value::String(format!("<text:{}>", s.chars().count())),
        other => other.clone(),
    }
}

/// Non-JSON response body (HTML error etc.): only the start, without long digit runs.
pub fn text_snippet(body: &[u8]) -> String {
    let s = String::from_utf8_lossy(&body[..body.len().min(300)]);
    scrub_digits(&s).replace('\n', " ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn path_without_phone_or_media() {
        assert_eq!(path("clients/wa/backups"), "clients/wa/backups");
        assert_eq!(path("clients/wa/backups/5511999998888/files"), "clients/wa/backups/<backup>/files");
        assert_eq!(
            path("clients/wa/backups/5511999998888/files/Databases/msgstore.db.crypt15"),
            "clients/wa/backups/<backup>/files/Databases/msgstore.db.crypt15"
        );
        assert_eq!(
            path("clients/wa/backups/5511999998888/files/Media/WhatsApp Images/IMG-20240101-WA0001.jpg"),
            "clients/wa/backups/<backup>/files/Media/….jpg"
        );
        assert_eq!(path("clients/wa/backups/1/files/abcdef0123.mcrypt1"), "clients/wa/backups/<backup>/files/….mcrypt1");
    }

    #[test]
    fn form_redacted() {
        let pairs = [
            ("Email", "someone@gmail.com"),
            ("Token", "oauth2_4/secret"),
            ("service", "ac2dm"),
            ("Error", "BadAuthentication"),
        ];
        let s = form(pairs);
        assert!(!s.contains("someone") && !s.contains("secret"), "{s}");
        assert!(s.contains("service=ac2dm") && s.contains("Error=BadAuthentication"), "{s}");
        assert!(s.contains("Email=<redacted:17>"), "{s}");
    }

    #[test]
    fn json_redacted() {
        let v = json!({
            "backups": [{
                "name": "clients/wa/backups/5511999998888",
                "updateTime": "2026-11-01T03:00:00Z",
                "sizeBytes": "123456",
                "metadata": "{\"encryptedBackupEnabled\":true,\"numOfMessages\":42,\"backupFrequency\":\"daily\",\"lastUploadedPhone\":\"5511999998888\"}"
            }],
            "nextPageToken": "secret",
            "error": {"message": "Backup 5511999998888 not found", "code": 404}
        });
        let s = json(&v).to_string();
        assert!(!s.contains("5511999998888"), "{s}");
        assert!(!s.contains("secret"), "{s}");
        assert!(s.contains("\"nextPageToken\":\"<text:6>\""), "{s}");
        assert!(s.contains("2026-11-01") && s.contains("numOfMessages") && s.contains("42") && s.contains("daily"), "{s}");
        assert!(json(&json!({"metadata": "{\"chatdbSize\":\"123456789\"}"})).to_string().contains("123456789"));
        assert!(s.contains("Backup <n> not found"), "{s}");
    }
}
