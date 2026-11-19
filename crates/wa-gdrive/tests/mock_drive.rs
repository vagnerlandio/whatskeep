//! Fluxo completo contra um servidor HTTP falso: login, listagem paginada,
//! download com MD5 e descriptografia do msgstore baixado.

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

/// Servidor HTTP/1.1 mínimo (uma requisição por conexão).
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

/// Google falso: só o WhatsApp comum tem token; o Business é recusado.
fn google(corrupt_md5: bool) -> (Endpoints, Arc<Mutex<Vec<Req>>>) {
    let md5 = if corrupt_md5 { md5_b64(b"outra coisa") } else { md5_b64(BACKUP) };
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
fn fluxo_completo() {
    let (ep, log) = google(false);
    let session = login(&ep, "a@b.com", OauthCookie::new("oauth2_4/cookie".into())).unwrap();
    assert_eq!(session.tokens.len(), 1);
    assert_eq!(session.tokens[0].0, DriveApp::WhatsApp);
    assert_eq!(session.refused.len(), 1);
    assert_eq!(session.refused[0].0, DriveApp::Business);

    let api = Api::new(&ep, &session.tokens[0].1).unwrap();
    let backups = api.backups().unwrap();
    assert_eq!(backups.len(), 2, "duas páginas");
    assert_eq!(backups[0].phone.as_deref(), Some("5511900000001"));
    assert_eq!((backups[0].encrypted, backups[0].chat_db_size), (Some(true), Some(1234)));
    assert!(backups[1].uploading);

    let files = api.files(&backups[0].name).unwrap();
    let db = find_msgstore(&files).unwrap();
    let mut seen = Vec::new();
    let bytes = api.download(db, &AtomicBool::new(false), |d, t| seen.push((d, t))).unwrap();
    assert_eq!(bytes, BACKUP);
    assert_eq!(seen.last(), Some(&(BACKUP.len() as u64, BACKUP.len() as u64)));

    // O que foi baixado abre com a chave do backup.
    let key = wa_crypt::BackupKey::from_hex(KEY_HEX).unwrap();
    let plain = wa_crypt::decrypt(&bytes, &key).unwrap();
    assert_eq!(plain.payload, wa_crypt::Payload::Sqlite);

    // O token mestre só foi para o endpoint de autenticação, e o token de
    // acesso só no cabeçalho Authorization.
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
fn cookie_recusado() {
    let (ep, _) = google(false);
    let err = login(&ep, "a@b.com", OauthCookie::new("expirado".into())).err().unwrap();
    assert!(matches!(err, Error::BadAuthentication), "{err:?}");
}

#[test]
fn md5_divergente_e_recusado() {
    let (ep, _) = google(true);
    let session = login(&ep, "a@b.com", OauthCookie::new("oauth2_4/cookie".into())).unwrap();
    let api = Api::new(&ep, &session.tokens[0].1).unwrap();
    let files = api.files("clients/wa/backups/5511900000001").unwrap();
    let err = api.download(find_msgstore(&files).unwrap(), &AtomicBool::new(false), |_, _| {}).err().unwrap();
    assert!(matches!(err, Error::Checksum), "{err:?}");
}

#[test]
fn cancelamento_token_expirado_e_redirecionamento() {
    let (ep, _) = google(false);
    let session = login(&ep, "a@b.com", OauthCookie::new("oauth2_4/cookie".into())).unwrap();
    let api = Api::new(&ep, &session.tokens[0].1).unwrap();
    let files = api.files("clients/wa/backups/5511900000001").unwrap();
    let db = find_msgstore(&files).unwrap();
    assert!(matches!(api.download(db, &AtomicBool::new(true), |_, _| {}), Err(Error::Cancelled)));

    let stale = wa_gdrive::AccessToken::new("ya29.VELHO".into());
    let api = Api::new(&ep, &stale).unwrap();
    assert!(matches!(api.backups(), Err(Error::TokenExpired)));

    // Redirecionamento não é seguido (o token não vai para outro host).
    let api = Api::new(&ep, &session.tokens[0].1).unwrap();
    assert!(matches!(api.files("clients/wa/backups/redirect"), Err(Error::Http(302))));
    // Nome de backup que tenta sair do caminho é recusado antes da rede.
    assert!(matches!(api.files("clients/wa/backups/../../x"), Err(Error::Unexpected(_))));
}
