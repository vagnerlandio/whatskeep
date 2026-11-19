//! Full flow against a fake HTTP server: login, paginated listing, download
//! with MD5 and decryption of the downloaded msgstore.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use md5::{Digest, Md5};
use wa_gdrive::{Api, DriveApp, Endpoints, Error, OauthCookie, find_msgstore, login};

const BACKUP: &[u8] = include_bytes!("../../wa-import/tests/fixtures/msgstore-modern.db.crypt15");
const KEY_HEX: &str = include_str!("../../wa-crypt/tests/fixtures/key15.hex");

#[derive(Clone, Debug)]
struct Req {
    method: String,
    target: String,
    auth: Option<String>,
    body: String,
}

struct Resp(u16, String, Vec<u8>);

fn ok(body: impl Into<Vec<u8>>) -> Resp {
    Resp(200, "text/plain".into(), body.into())
}

/// Minimal HTTP/1.1 server (one request per connection).
fn serve(route: impl Fn(&Req) -> Resp + Send + 'static) -> (String, Arc<Mutex<Vec<Req>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let log = Arc::new(Mutex::new(Vec::new()));
    let log2 = log.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let mut stream = stream.unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            let mut parts = line.split_whitespace();
            let method = parts.next().unwrap_or_default().to_owned();
            let target = parts.next().unwrap_or_default().to_owned();
            let (mut len, mut auth) = (0usize, None);
            loop {
                let mut h = String::new();
                reader.read_line(&mut h).unwrap();
                let h = h.trim_end();
                if h.is_empty() {
                    break;
                }
                let (k, v) = h.split_once(':').unwrap();
                match k.to_ascii_lowercase().as_str() {
                    "content-length" => len = v.trim().parse().unwrap(),
                    "authorization" => auth = Some(v.trim().to_owned()),
                    _ => {}
                }
            }
            let mut body = vec![0; len];
            reader.read_exact(&mut body).unwrap();
            let req = Req { method, target, auth, body: String::from_utf8_lossy(&body).into_owned() };
            log2.lock().unwrap().push(req.clone());
            let Resp(status, ctype, body) = route(&req);
            let extra = if status == 302 { "Location: http://example.invalid/\r\n" } else { "" };
            let head = format!(
                "HTTP/1.1 {status} X\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\n{extra}Connection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(&body);
        }
    });
    (base, log)
}

fn form(body: &str) -> Vec<(String, String)> {
    url::form_urlencoded::parse(body.as_bytes()).into_owned().collect()
}

fn field<'a>(f: &'a [(String, String)], k: &str) -> Option<&'a str> {
    f.iter().find(|(key, _)| key == k).map(|(_, v)| v.as_str())
}

fn md5_b64(data: &[u8]) -> String {
    B64.encode(Md5::digest(data))
}

/// Fake Google: only regular WhatsApp gets a token; Business is refused.
fn google(corrupt_md5: bool) -> (Endpoints, Arc<Mutex<Vec<Req>>>) {
    let md5 = if corrupt_md5 { md5_b64(b"something else") } else { md5_b64(BACKUP) };
    let (base, log) = serve(move |r| {
        let f = form(&r.body);
        match (r.method.as_str(), r.target.as_str()) {
            ("POST", "/auth") if field(&f, "ACCESS_TOKEN") == Some("1") => {
                if field(&f, "Token") == Some("oauth2_4/cookie") && field(&f, "Email") == Some("a@b.com") {
                    ok("SID=x\nToken=aas_et/MASTER\nservices=mail\n")
                } else {
                    ok("Error=BadAuthentication\n")
                }
            }
            ("POST", "/auth") => match (field(&f, "Token"), field(&f, "app")) {
                (Some("aas_et/MASTER"), Some("com.whatsapp")) => ok("Auth=ya29.WA\nExpiry=3600\n"),
                _ => ok("Error=BadAuthentication\n"),
            },
            ("GET", _) if r.auth.as_deref() != Some("Bearer ya29.WA") => Resp(401, "text/plain".into(), vec![]),
            ("GET", t) if t.starts_with("/v1/clients/wa/backups?") && !t.contains("pageToken") => Resp(
                200,
                "application/json".into(),
                br#"{"backups":[{"name":"clients/wa/backups/5511900000001","updateTime":"2026-11-01T10:00:00.000Z","sizeBytes":"999","metadata":"{\"encryptedBackupEnabled\":true,\"chatdbSize\":\"1234\"}"}],"nextPageToken":"p2"}"#.to_vec(),
            ),
            ("GET", t) if t.starts_with("/v1/clients/wa/backups?") => Resp(
                200,
                "application/json".into(),
                br#"{"backups":[{"name":"clients/wa/backups/5511900000002","activeTransactionId":"t"}]}"#.to_vec(),
            ),
            ("GET", t) if t.starts_with("/v1/clients/wa/backups/5511900000001/files?") => Resp(
                200,
                "application/json".into(),
                format!(
                    r#"{{"files":[
                      {{"name":"clients/wa/backups/5511900000001/files/Media/ab12.mcrypt1","sizeBytes":"10"}},
                      {{"name":"clients/wa/backups/5511900000001/files/Databases/msgstore.db.crypt15","sizeBytes":"{}","md5Hash":"{md5}"}}
                    ]}}"#,
                    BACKUP.len()
                )
                .into_bytes(),
            ),
            ("GET", "/v1/clients/wa/backups/5511900000001/files/Databases/msgstore.db.crypt15?alt=media") => {
                Resp(200, "application/octet-stream".into(), BACKUP.to_vec())
            }
            ("GET", t) if t.contains("redirect") => Resp(302, "text/plain".into(), vec![]),
            _ => Resp(404, "text/plain".into(), vec![]),
        }
    });
    (Endpoints { auth: format!("{base}/auth"), backup: format!("{base}/v1") }, log)
}

#[test]
fn full_flow() {
    let (ep, log) = google(false);
    let session = login(&ep, "a@b.com", OauthCookie::new("oauth2_4/cookie".into())).unwrap();
    assert_eq!(session.tokens.len(), 1);
    assert_eq!(session.tokens[0].0, DriveApp::WhatsApp);
    assert_eq!(session.refused.len(), 1);
    assert_eq!(session.refused[0].0, DriveApp::Business);

    let api = Api::new(&ep, &session.tokens[0].1).unwrap();
    let backups = api.backups().unwrap();
    assert_eq!(backups.len(), 2, "two pages");
    assert_eq!(backups[0].phone.as_deref(), Some("5511900000001"));
    assert_eq!((backups[0].encrypted, backups[0].chat_db_size), (Some(true), Some(1234)));
    assert!(backups[1].uploading);

    let files = api.files(&backups[0].name).unwrap();
    let db = find_msgstore(&files).unwrap();
    let mut seen = Vec::new();
    let bytes = api.download(db, &AtomicBool::new(false), |d, t| seen.push((d, t))).unwrap();
    assert_eq!(bytes, BACKUP);
    assert_eq!(seen.last(), Some(&(BACKUP.len() as u64, BACKUP.len() as u64)));

    // What was downloaded opens with the backup key.
    let key = wa_crypt::BackupKey::from_hex(KEY_HEX).unwrap();
    let plain = wa_crypt::decrypt(&bytes, &key).unwrap();
    assert_eq!(plain.payload, wa_crypt::Payload::Sqlite);

    // The master token only went to the authentication endpoint, and the
    // access token only in the Authorization header.
    let log = log.lock().unwrap();
    for r in log.iter() {
        if r.target != "/auth" {
            assert!(!r.target.contains("MASTER") && !r.target.contains("ya29") && r.body.is_empty(), "{r:?}");
            assert_eq!(r.auth.as_deref(), Some("Bearer ya29.WA"));
        }
    }
    let auth_reqs: Vec<_> = log.iter().filter(|r| r.target == "/auth").map(|r| form(&r.body)).collect();
    assert_eq!(field(&auth_reqs[1], "service"), Some("oauth2:https://www.googleapis.com/auth/drive.appdata"));
    assert_eq!(field(&auth_reqs[1], "client_sig"), Some("38a0f7d505fe18fec64fbf343ecaaaf310dbd799"));
    let android_id = field(&auth_reqs[0], "androidId").unwrap();
    assert!(android_id.len() == 16 && auth_reqs.iter().all(|f| field(f, "androidId") == Some(android_id)));
}

#[test]
fn cookie_refused() {
    let (ep, _) = google(false);
    let err = login(&ep, "a@b.com", OauthCookie::new("expired".into())).err().unwrap();
    assert!(matches!(err, Error::BadAuthentication), "{err:?}");
}

#[test]
fn md5_mismatch_is_refused() {
    let (ep, _) = google(true);
    let session = login(&ep, "a@b.com", OauthCookie::new("oauth2_4/cookie".into())).unwrap();
    let api = Api::new(&ep, &session.tokens[0].1).unwrap();
    let files = api.files("clients/wa/backups/5511900000001").unwrap();
    let err = api.download(find_msgstore(&files).unwrap(), &AtomicBool::new(false), |_, _| {}).err().unwrap();
    assert!(matches!(err, Error::Checksum), "{err:?}");
}

#[test]
fn cancellation_expired_token_and_redirect() {
    let (ep, _) = google(false);
    let session = login(&ep, "a@b.com", OauthCookie::new("oauth2_4/cookie".into())).unwrap();
    let api = Api::new(&ep, &session.tokens[0].1).unwrap();
    let files = api.files("clients/wa/backups/5511900000001").unwrap();
    let db = find_msgstore(&files).unwrap();
    assert!(matches!(api.download(db, &AtomicBool::new(true), |_, _| {}), Err(Error::Cancelled)));

    let stale = wa_gdrive::AccessToken::new("ya29.STALE".into());
    let api = Api::new(&ep, &stale).unwrap();
    assert!(matches!(api.backups(), Err(Error::TokenExpired)));

    // Redirects are not followed (the token never goes to another host).
    let api = Api::new(&ep, &session.tokens[0].1).unwrap();
    assert!(matches!(api.files("clients/wa/backups/redirect"), Err(Error::Http(302))));
    // A backup name that tries to escape the path is refused before any network call.
    assert!(matches!(api.files("clients/wa/backups/../../x"), Err(Error::Unexpected(_))));
}

/// The diagnostics log (requests/responses) shows the flow, but never the
/// cookie, the tokens, the e-mail, the device id or the backup's phone number.
#[test]
fn request_log_leaks_no_secrets() {
    #[derive(Clone, Default)]
    struct Buf(Arc<Mutex<Vec<u8>>>);
    impl Write for Buf {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let buf = Buf::default();
    let writer = buf.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_ansi(false)
        .with_writer(move || writer.clone())
        .finish();

    // Global (not `with_default`): with tests running in parallel, a scoped
    // subscriber loses events because of the callsite interest cache. This way
    // the logs of the other tests in this file also go through the check.
    tracing::subscriber::set_global_default(subscriber).unwrap();

    let (ep, reqs) = google(false);
    let session = login(&ep, "a@b.com", OauthCookie::new("oauth2_4/cookie".into())).unwrap();
    let api = Api::new(&ep, &session.tokens[0].1).unwrap();
    let backups = api.backups().unwrap();
    let files = api.files(&backups[0].name).unwrap();
    api.download(find_msgstore(&files).unwrap(), &AtomicBool::new(false), |_, _| {}).unwrap();

    let log = String::from_utf8(buf.0.lock().unwrap().clone()).unwrap();
    if std::env::var_os("WA_SHOW_LOG").is_some() {
        eprintln!("{log}");
    }
    let android_id = form(&reqs.lock().unwrap()[0].body)
        .into_iter()
        .find(|(k, _)| k == "androidId")
        .map(|(_, v)| v)
        .unwrap();
    for secret in ["oauth2_4/cookie", "aas_et", "MASTER", "ya29", "a@b.com", "5511900000001", "5511900000002", "p2", &android_id] {
        assert!(!log.contains(secret), "the log contains {secret:?}:\n{log}");
    }
    // But it shows what matters for diagnosing.
    for expected in [
        "→ POST",
        "Token=<redacted:",
        "service=ac2dm",
        "Error=",
        "→ GET",
        "clients/wa/backups/<backup>/files",
        "Databases/msgstore.db.crypt15",
        "status=200",
        "encryptedBackupEnabled",
        "downloaded",
    ] {
        assert!(log.contains(expected), "the log does not contain {expected:?}:\n{log}");
    }
}
