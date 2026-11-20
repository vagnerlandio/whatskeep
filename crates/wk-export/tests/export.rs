use std::fs;
use std::path::Path;
use std::sync::atomic::AtomicBool;

use rusqlite::Connection;
use serde_json::Value;
use wk_export::format::{self, Kdf, Manifest};
use wk_export::{ChatEntry, Error, Group, MediaMode, Options, SlotContent, export, with_csp};
use wk_import::SourceApp;
use wk_vault::{KdfParams, Vault};
use zeroize::Zeroizing;

const SHELL: &str = "<!doctype html><html><head><meta charset=\"UTF-8\"><title>x</title><script type=\"module\">console.log(1)</script></head><body></body></html>";
const PHOTO: &[u8] = b"\xFF\xD8\xFF\xE0 beach photo at full resolution";

struct Fixture {
    _dir: tempfile::TempDir,
    vault: Vault,
}

impl Fixture {
    fn new() -> Self {
        Self::with_photo(PHOTO, "image/jpeg")
    }

    fn with_photo(photo: &[u8], mime: &str) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::create(&dir.path().join("vault"), "vault password", KdfParams::MINIMUM).unwrap();
        let src = Connection::open_in_memory().unwrap();
        src.execute_batch(include_str!("../../wk-import/tests/fixtures/modern.sql")).unwrap();
        vault.store(SourceApp::WhatsApp).unwrap().import(&src).unwrap();
        let w = vault.media(SourceApp::WhatsApp).unwrap().writer().unwrap();
        w.put(photo, None, Some(mime), Some("Media/WhatsApp Images/IMG-0001.jpg")).unwrap();
        Self { _dir: dir, vault }
    }

    fn conn(&self) -> Connection {
        self.vault.store(SourceApp::WhatsApp).unwrap().open_read_only().unwrap().unwrap()
    }

    fn chat_ids(&self) -> (i64, i64) {
        let chats = wk_vault::query::chats(&self.conn()).unwrap();
        (chats.iter().find(|c| c.kind == "direct").unwrap().id, chats.iter().find(|c| c.kind == "group").unwrap().id)
    }

    fn export(&self, dest: &Path, groups: Vec<(&str, Vec<i64>)>, media: MediaMode) -> Result<wk_export::Summary, Error> {
        let reader = self.vault.media(SourceApp::WhatsApp).unwrap().reader().unwrap();
        let opts = Options {
            groups: groups.into_iter().map(|(p, chats)| Group { password: Zeroizing::new(p.into()), chats }).collect(),
            media,
            kdf: Kdf::TEST,
        };
        export(&self.conn(), reader.as_ref(), SHELL, dest, &opts, &AtomicBool::new(false), |_| {})
    }
}

// ---------------------------------------------------------------- reader (like the browser)

fn manifest(dir: &Path) -> Manifest {
    let js = fs::read_to_string(dir.join("data/manifest.js")).unwrap();
    let json = js.trim().strip_prefix("window.__wkManifest = ").unwrap().strip_suffix(';').unwrap();
    serde_json::from_str(json).unwrap()
}

/// Slots the password opens.
fn unlock(dir: &Path, password: &str) -> Vec<SlotContent> {
    let m = manifest(dir);
    let salt = format::unb64(&m.kdf.salt).unwrap();
    let key = format::password_key(password, &salt, Kdf { m: m.kdf.m, t: m.kdf.t, p: m.kdf.p }).unwrap();
    m.slots
        .iter()
        .filter_map(|s| {
            let plain = format::open(&key, format::SLOT_AAD, &format::unb64(&s.n).unwrap(), &format::unb64(&s.c).unwrap()).ok()?;
            Some(serde_json::from_slice(&plain).unwrap())
        })
        .collect()
}

fn salt(dir: &Path) -> Vec<u8> {
    format::unb64(&manifest(dir).kdf.salt).unwrap()
}

fn chat_key(dir: &Path, slot: &SlotContent, entry: &ChatEntry) -> format::Key {
    let master: [u8; 32] = format::unb64(&slot.master).unwrap().try_into().unwrap();
    format::chat_key(&master, &salt(dir), &entry.key_id)
}

fn read_chunk(dir: &Path, key: &[u8; 32], id: &str) -> Result<Vec<Value>, Error> {
    let js = fs::read_to_string(dir.join(format!("data/{id}.js"))).unwrap();
    let b64 = js.split('"').nth(3).unwrap();
    let blob = format::unb64(b64).unwrap();
    let plain = format::open(key, id.as_bytes(), &blob[..12], &blob[12..])?;
    let json = format::unpack_chunk(&plain)?;
    let v: Value = serde_json::from_slice(&json).unwrap();
    Ok(v["messages"].as_array().unwrap().clone())
}

fn messages(dir: &Path, slot: &SlotContent, entry: &ChatEntry) -> Vec<Value> {
    let key = chat_key(dir, slot, entry);
    entry.chunks.iter().flat_map(|c| read_chunk(dir, &key, c).unwrap()).collect()
}

fn all_files(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    for e in fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() { out.extend(all_files(&p)) } else { out.push(p) }
    }
    out
}

// ---------------------------------------------------------------- tests

#[test]
fn one_password_all_chats_with_encrypted_media() {
    let f = Fixture::new();
    let (alice, group) = f.chat_ids();
    let out = tempfile::tempdir().unwrap();
    let s = f.export(out.path(), vec![("export password", vec![alice, group])], MediaMode::Encrypted).unwrap();
    assert_eq!((s.chats, s.slots, s.media_files), (2, 1, 1));
    assert_eq!(s.messages, 12);

    let slots = unlock(out.path(), "export password");
    assert_eq!(slots.len(), 1);
    let entry = slots[0].chats.iter().find(|c| c.chat["id"] == alice).unwrap();
    let msgs = messages(out.path(), &slots[0], entry);
    assert_eq!(msgs.len(), 9);
    assert!(msgs.iter().any(|m| m["text"] == "Oi, tudo bem? Vamos tomar um café"));
    // Internal fields (hash and file path on the phone) do not go into the package.
    let photo = msgs.iter().find(|m| m["kind"] == "image").unwrap();
    assert!(photo["media"].get("fileHash").is_none() && photo["media"].get("filePath").is_none());
    assert_eq!(photo["media"]["available"], true);
    assert!(photo["media"]["thumb"].is_string());

    // The media opens with the key derived from the chat.
    let r = &photo["media"]["ref"];
    let key = format::media_key(&chat_key(out.path(), &slots[0], entry), &salt(out.path()), r["id"].as_str().unwrap());
    let pack = fs::read(out.path().join(format!("media/pack-{:03}.bin", r["p"].as_u64().unwrap()))).unwrap();
    let (o, n) = (r["o"].as_u64().unwrap() as usize, r["n"].as_u64().unwrap());
    let prefix: [u8; 7] = format::unb64(r["np"].as_str().unwrap()).unwrap().try_into().unwrap();
    let ct = &pack[o..o + format::stream_len(n) as usize];
    assert_eq!(format::open_stream(&key, &prefix, r["id"].as_str().unwrap(), ct, n).unwrap(), PHOTO);
    // Another chat's key does not open it.
    let other = slots[0].chats.iter().find(|c| c.chat["id"] == group).unwrap();
    let wrong = format::media_key(&chat_key(out.path(), &slots[0], other), &salt(out.path()), r["id"].as_str().unwrap());
    assert!(format::open_stream(&wrong, &prefix, r["id"].as_str().unwrap(), ct, n).is_err());
}

#[test]
fn password_per_chat_isolates() {
    let f = Fixture::new();
    let (alice, group) = f.chat_ids();
    let out = tempfile::tempdir().unwrap();
    f.export(out.path(), vec![("alice password", vec![alice]), ("group password", vec![group])], MediaMode::None)
        .unwrap();

    let a = unlock(out.path(), "alice password");
    assert_eq!(a.len(), 1);
    assert_eq!(a[0].chats.len(), 1);
    assert_eq!(a[0].chats[0].chat["id"], alice);
    let g = unlock(out.path(), "group password");
    assert_eq!(g[0].chats[0].chat["id"], group);
    assert!(unlock(out.path(), "wrong password!!").is_empty());

    // Alice's key does not open the group's chunks.
    let alice_key = chat_key(out.path(), &a[0], &a[0].chats[0]);
    assert!(matches!(read_chunk(out.path(), &alice_key, &g[0].chats[0].chunks[0]), Err(Error::Authentication)));
}

#[test]
fn partial_export_excludes_other_chats() {
    let f = Fixture::new();
    let (alice, _) = f.chat_ids();
    let out = tempfile::tempdir().unwrap();
    f.export(out.path(), vec![("export password", vec![alice])], MediaMode::Encrypted).unwrap();
    let slots = unlock(out.path(), "export password");
    assert_eq!(slots[0].chats.len(), 1);
    let texts: Vec<String> = messages(out.path(), &slots[0], &slots[0].chats[0])
        .iter()
        .filter_map(|m| m["text"].as_str().map(str::to_owned))
        .collect();
    assert!(!texts.iter().any(|t| t.contains("grupo")), "{texts:?}");
}

#[test]
fn no_plaintext_or_phone_numbers_in_files() {
    let f = Fixture::new();
    let (alice, group) = f.chat_ids();
    for mode in [MediaMode::Encrypted, MediaMode::None] {
        let out = tempfile::tempdir().unwrap();
        f.export(out.path(), vec![("export password", vec![alice, group])], mode).unwrap();
        for path in all_files(out.path()) {
            let name = path.to_string_lossy().to_string();
            let bytes = fs::read(&path).unwrap();
            for needle in ["Vamos tomar um caf", "5511911111111", "5511922222222", "whatsapp.net", "Família", "IMG-0001", "beach photo"] {
                assert!(!name.contains(needle), "name {name}");
                assert!(!bytes.windows(needle.len()).any(|w| w == needle.as_bytes()), "{name} contains {needle}");
            }
            assert!(!bytes.windows(PHOTO.len()).any(|w| w == PHOTO), "{name} has the photo in plaintext");
        }
    }
}

#[test]
fn plaintext_media_when_chosen() {
    let f = Fixture::new();
    let (alice, _) = f.chat_ids();
    let out = tempfile::tempdir().unwrap();
    f.export(out.path(), vec![("export password", vec![alice])], MediaMode::Plain).unwrap();
    assert!(out.path().join(".metadata_never_index").is_file());
    let slots = unlock(out.path(), "export password");
    let msgs = messages(out.path(), &slots[0], &slots[0].chats[0]);
    let r = &msgs.iter().find(|m| m["kind"] == "image").unwrap()["media"]["ref"];
    let file = r["f"].as_str().unwrap();
    assert!(file.starts_with("media/") && !file.contains('.'), "random name without extension: {file}");
    assert_eq!(fs::read(out.path().join(file)).unwrap(), PHOTO);
}

#[test]
fn tampered_chunk_fails() {
    let f = Fixture::new();
    let (alice, _) = f.chat_ids();
    let out = tempfile::tempdir().unwrap();
    f.export(out.path(), vec![("export password", vec![alice])], MediaMode::None).unwrap();
    let slots = unlock(out.path(), "export password");
    let entry = &slots[0].chats[0];
    let path = out.path().join(format!("data/{}.js", entry.chunks[0]));
    let js = fs::read_to_string(&path).unwrap();
    let b64 = js.split('"').nth(3).unwrap().to_owned();
    let mut blob = format::unb64(&b64).unwrap();
    blob[40] ^= 1;
    fs::write(&path, js.replace(&b64, &base64_encode(&blob))).unwrap();
    assert!(matches!(read_chunk(out.path(), &chat_key(out.path(), &slots[0], entry), &entry.chunks[0]), Err(Error::Authentication)));

    // An intact chunk served under another name also fails (the id is the AAD).
    fs::write(&path, &js).unwrap();
    let renamed = out.path().join("data/0000000000000000.js");
    fs::copy(&path, &renamed).unwrap();
    let key = chat_key(out.path(), &slots[0], entry);
    assert!(read_chunk(out.path(), &key, &entry.chunks[0]).is_ok());
    assert!(matches!(read_chunk(out.path(), &key, "0000000000000000"), Err(Error::Authentication)));
}

fn base64_encode(b: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(b)
}

#[test]
fn validations() {
    let f = Fixture::new();
    let (alice, group) = f.chat_ids();
    let out = tempfile::tempdir().unwrap();
    assert!(matches!(f.export(out.path(), vec![("short", vec![alice])], MediaMode::None), Err(Error::Invalid(_))));
    assert!(matches!(f.export(out.path(), vec![("export password", vec![])], MediaMode::None), Err(Error::Invalid(_))));
    assert!(matches!(
        f.export(out.path(), vec![("password one 123", vec![alice]), ("password two 123", vec![alice, group])], MediaMode::None),
        Err(Error::Invalid(_))
    ));
    assert!(matches!(f.export(out.path(), vec![("export password", vec![999])], MediaMode::None), Err(Error::Invalid(_))));
    fs::write(out.path().join("other.txt"), "x").unwrap();
    assert!(matches!(
        f.export(out.path(), vec![("export password", vec![alice])], MediaMode::None),
        Err(Error::DestinationNotEmpty)
    ));
}

#[test]
fn csp_with_script_hashes() {
    let html = with_csp(SHELL);
    assert!(html.contains("connect-src 'none'"));
    // 'self' on file:// opens up the whole network in WebKit (tested in Safari/Playwright).
    assert!(!html.contains("'self'"), "{html}");
    assert!(html.contains("form-action 'none'"));
    // sha256 of "console.log(1)"
    assert!(html.contains("'sha256-"), "{html}");
    let head = html.find("<head>").unwrap();
    assert!(html[head..].find("Content-Security-Policy").unwrap() < html[head..].find("<script").unwrap());
}

#[test]
fn chunks_have_rounded_size() {
    let f = Fixture::new();
    let (alice, _) = f.chat_ids();
    let out = tempfile::tempdir().unwrap();
    f.export(out.path(), vec![("export password", vec![alice])], MediaMode::None).unwrap();
    let slots = unlock(out.path(), "export password");
    let js = fs::read_to_string(out.path().join(format!("data/{}.js", slots[0].chats[0].chunks[0]))).unwrap();
    let blob = format::unb64(js.split('"').nth(3).unwrap()).unwrap();
    assert_eq!((blob.len() - 12 - 16) % format::PAD, 0);
}

/// Generates real packages for the browser test (Playwright):
/// `WK_EXPORT_OUT=<dir> WK_EXPORT_SHELL=<index.html> WK_EXPORT_IMAGE=<png> cargo test -p wk-export -- --ignored generate_browser_packages`
///
/// The package names and passwords below are matched by
/// `packages/export-shell/e2e/export.e2e.cjs`; keep both in sync.
#[test]
#[ignore]
fn generate_browser_packages() {
    let (Ok(out), Ok(shell), Ok(image)) =
        (std::env::var("WK_EXPORT_OUT"), std::env::var("WK_EXPORT_SHELL"), std::env::var("WK_EXPORT_IMAGE"))
    else {
        return;
    };
    let shell = fs::read_to_string(shell).unwrap();
    let image = fs::read(image).unwrap();
    let f = Fixture::with_photo(&image, "image/png");
    let (alice, group) = f.chat_ids();
    let reader = f.vault.media(SourceApp::WhatsApp).unwrap().reader().unwrap();
    for (name, groups, media) in [
        ("cifrado", vec![("senha do export", vec![alice, group])], MediaMode::Encrypted),
        ("claro", vec![("senha do export", vec![alice, group])], MediaMode::Plain),
        ("por-conversa", vec![("senha da alice", vec![alice]), ("senha do grupo", vec![group])], MediaMode::None),
    ] {
        let dest = Path::new(&out).join(name);
        let _ = fs::remove_dir_all(&dest);
        let opts = Options {
            groups: groups.into_iter().map(|(p, c)| Group { password: Zeroizing::new(p.into()), chats: c }).collect(),
            media,
            kdf: Kdf::DEFAULT,
        };
        export(&f.conn(), reader.as_ref(), &shell, &dest, &opts, &AtomicBool::new(false), |_| {}).unwrap();
    }
}
