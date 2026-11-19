use std::fs;
use std::path::Path;
use std::sync::atomic::AtomicBool;

use rusqlite::Connection;
use serde_json::Value;
use wa_export::format::{self, Kdf, Manifest};
use wa_export::{ChatEntry, Error, Group, MediaMode, Options, SlotContent, export, with_csp};
use wa_import::SourceApp;
use wa_vault::{KdfParams, Vault};
use zeroize::Zeroizing;

const SHELL: &str = "<!doctype html><html><head><meta charset=\"UTF-8\"><title>x</title><script type=\"module\">console.log(1)</script></head><body></body></html>";
const PHOTO: &[u8] = b"\xFF\xD8\xFF\xE0 foto da praia em resolucao total";

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
        let vault = Vault::create(&dir.path().join("vault"), "senha do cofre", KdfParams::MINIMUM).unwrap();
        let src = Connection::open_in_memory().unwrap();
        src.execute_batch(include_str!("../../wa-import/tests/fixtures/modern.sql")).unwrap();
        vault.store(SourceApp::WhatsApp).unwrap().import(&src).unwrap();
        let w = vault.media(SourceApp::WhatsApp).unwrap().writer().unwrap();
        w.put(photo, None, Some(mime), Some("Media/WhatsApp Images/IMG-0001.jpg")).unwrap();
        Self { _dir: dir, vault }
    }

    fn conn(&self) -> Connection {
        self.vault.store(SourceApp::WhatsApp).unwrap().open_read_only().unwrap().unwrap()
    }

    fn chat_ids(&self) -> (i64, i64) {
        let chats = wa_vault::query::chats(&self.conn()).unwrap();
        (chats.iter().find(|c| c.kind == "direct").unwrap().id, chats.iter().find(|c| c.kind == "group").unwrap().id)
    }

    fn export(&self, dest: &Path, groups: Vec<(&str, Vec<i64>)>, media: MediaMode) -> Result<wa_export::Summary, Error> {
        let reader = self.vault.media(SourceApp::WhatsApp).unwrap().reader().unwrap();
        let opts = Options {
            groups: groups.into_iter().map(|(p, chats)| Group { password: Zeroizing::new(p.into()), chats }).collect(),
            media,
            kdf: Kdf::TEST,
        };
        export(&self.conn(), reader.as_ref(), SHELL, dest, &opts, &AtomicBool::new(false), |_| {})
    }
}

// ---------------------------------------------------------------- leitor (como o navegador)

fn manifest(dir: &Path) -> Manifest {
    let js = fs::read_to_string(dir.join("data/manifest.js")).unwrap();
    let json = js.trim().strip_prefix("window.__waManifest = ").unwrap().strip_suffix(';').unwrap();
    serde_json::from_str(json).unwrap()
}

/// Slots que a senha abre.
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

// ---------------------------------------------------------------- testes

#[test]
fn uma_senha_todas_as_conversas_com_midia_cifrada() {
    let f = Fixture::new();
    let (alice, group) = f.chat_ids();
    let out = tempfile::tempdir().unwrap();
    let s = f.export(out.path(), vec![("senha do export", vec![alice, group])], MediaMode::Encrypted).unwrap();
    assert_eq!((s.chats, s.slots, s.media_files), (2, 1, 1));
    assert_eq!(s.messages, 12);

    let slots = unlock(out.path(), "senha do export");
    assert_eq!(slots.len(), 1);
    let entry = slots[0].chats.iter().find(|c| c.chat["id"] == alice).unwrap();
    let msgs = messages(out.path(), &slots[0], entry);
    assert_eq!(msgs.len(), 9);
    assert!(msgs.iter().any(|m| m["text"] == "Oi, tudo bem? Vamos tomar um café"));
    // Campos internos (hash e caminho do arquivo no celular) não vão para o pacote.
    let photo = msgs.iter().find(|m| m["kind"] == "image").unwrap();
    assert!(photo["media"].get("fileHash").is_none() && photo["media"].get("filePath").is_none());
    assert_eq!(photo["media"]["available"], true);
    assert!(photo["media"]["thumb"].is_string());

    // A mídia abre com a chave derivada da conversa.
    let r = &photo["media"]["ref"];
    let key = format::media_key(&chat_key(out.path(), &slots[0], entry), &salt(out.path()), r["id"].as_str().unwrap());
    let pack = fs::read(out.path().join(format!("media/pack-{:03}.bin", r["p"].as_u64().unwrap()))).unwrap();
    let (o, n) = (r["o"].as_u64().unwrap() as usize, r["n"].as_u64().unwrap());
    let prefix: [u8; 7] = format::unb64(r["np"].as_str().unwrap()).unwrap().try_into().unwrap();
    let ct = &pack[o..o + format::stream_len(n) as usize];
    assert_eq!(format::open_stream(&key, &prefix, r["id"].as_str().unwrap(), ct, n).unwrap(), PHOTO);
    // A chave de outra conversa não abre.
    let other = slots[0].chats.iter().find(|c| c.chat["id"] == group).unwrap();
    let wrong = format::media_key(&chat_key(out.path(), &slots[0], other), &salt(out.path()), r["id"].as_str().unwrap());
    assert!(format::open_stream(&wrong, &prefix, r["id"].as_str().unwrap(), ct, n).is_err());
}

#[test]
fn senha_por_conversa_isola() {
    let f = Fixture::new();
    let (alice, group) = f.chat_ids();
    let out = tempfile::tempdir().unwrap();
    f.export(out.path(), vec![("senha da alice", vec![alice]), ("senha do grupo", vec![group])], MediaMode::None)
        .unwrap();

    let a = unlock(out.path(), "senha da alice");
    assert_eq!(a.len(), 1);
    assert_eq!(a[0].chats.len(), 1);
    assert_eq!(a[0].chats[0].chat["id"], alice);
    let g = unlock(out.path(), "senha do grupo");
    assert_eq!(g[0].chats[0].chat["id"], group);
    assert!(unlock(out.path(), "senha errada!!").is_empty());

    // A chave da Alice não abre os pedaços do grupo.
    let alice_key = chat_key(out.path(), &a[0], &a[0].chats[0]);
    assert!(matches!(read_chunk(out.path(), &alice_key, &g[0].chats[0].chunks[0]), Err(Error::Authentication)));
}

#[test]
fn export_parcial_nao_inclui_outras_conversas() {
    let f = Fixture::new();
    let (alice, _) = f.chat_ids();
    let out = tempfile::tempdir().unwrap();
    f.export(out.path(), vec![("senha do export", vec![alice])], MediaMode::Encrypted).unwrap();
    let slots = unlock(out.path(), "senha do export");
    assert_eq!(slots[0].chats.len(), 1);
    let texts: Vec<String> = messages(out.path(), &slots[0], &slots[0].chats[0])
        .iter()
        .filter_map(|m| m["text"].as_str().map(str::to_owned))
        .collect();
    assert!(!texts.iter().any(|t| t.contains("grupo")), "{texts:?}");
}

#[test]
fn nada_em_claro_nem_telefone_nos_arquivos() {
    let f = Fixture::new();
    let (alice, group) = f.chat_ids();
    for mode in [MediaMode::Encrypted, MediaMode::None] {
        let out = tempfile::tempdir().unwrap();
        f.export(out.path(), vec![("senha do export", vec![alice, group])], mode).unwrap();
        for path in all_files(out.path()) {
            let name = path.to_string_lossy().to_string();
            let bytes = fs::read(&path).unwrap();
            for needle in ["Vamos tomar um caf", "5511911111111", "5511922222222", "whatsapp.net", "Família", "IMG-0001", "foto da praia"] {
                assert!(!name.contains(needle), "nome {name}");
                assert!(!bytes.windows(needle.len()).any(|w| w == needle.as_bytes()), "{name} contém {needle}");
            }
            assert!(!bytes.windows(PHOTO.len()).any(|w| w == PHOTO), "{name} tem a foto em claro");
        }
    }
}

#[test]
fn midia_em_claro_quando_escolhido() {
    let f = Fixture::new();
    let (alice, _) = f.chat_ids();
    let out = tempfile::tempdir().unwrap();
    f.export(out.path(), vec![("senha do export", vec![alice])], MediaMode::Plain).unwrap();
    assert!(out.path().join(".metadata_never_index").is_file());
    let slots = unlock(out.path(), "senha do export");
    let msgs = messages(out.path(), &slots[0], &slots[0].chats[0]);
    let r = &msgs.iter().find(|m| m["kind"] == "image").unwrap()["media"]["ref"];
    let file = r["f"].as_str().unwrap();
    assert!(file.starts_with("media/") && !file.contains('.'), "nome aleatório sem extensão: {file}");
    assert_eq!(fs::read(out.path().join(file)).unwrap(), PHOTO);
}

#[test]
fn adulteracao_de_pedaco_falha() {
    let f = Fixture::new();
    let (alice, _) = f.chat_ids();
    let out = tempfile::tempdir().unwrap();
    f.export(out.path(), vec![("senha do export", vec![alice])], MediaMode::None).unwrap();
    let slots = unlock(out.path(), "senha do export");
    let entry = &slots[0].chats[0];
    let path = out.path().join(format!("data/{}.js", entry.chunks[0]));
    let js = fs::read_to_string(&path).unwrap();
    let b64 = js.split('"').nth(3).unwrap().to_owned();
    let mut blob = format::unb64(&b64).unwrap();
    blob[40] ^= 1;
    fs::write(&path, js.replace(&b64, &base64_encode(&blob))).unwrap();
    assert!(matches!(read_chunk(out.path(), &chat_key(out.path(), &slots[0], entry), &entry.chunks[0]), Err(Error::Authentication)));

    // Um pedaço íntegro servido com outro nome também falha (o id é o AAD).
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
fn validacoes() {
    let f = Fixture::new();
    let (alice, group) = f.chat_ids();
    let out = tempfile::tempdir().unwrap();
    assert!(matches!(f.export(out.path(), vec![("curta", vec![alice])], MediaMode::None), Err(Error::Invalid(_))));
    assert!(matches!(f.export(out.path(), vec![("senha do export", vec![])], MediaMode::None), Err(Error::Invalid(_))));
    assert!(matches!(
        f.export(out.path(), vec![("senha um 123", vec![alice]), ("senha dois 123", vec![alice, group])], MediaMode::None),
        Err(Error::Invalid(_))
    ));
    assert!(matches!(f.export(out.path(), vec![("senha do export", vec![999])], MediaMode::None), Err(Error::Invalid(_))));
    fs::write(out.path().join("outro.txt"), "x").unwrap();
    assert!(matches!(
        f.export(out.path(), vec![("senha do export", vec![alice])], MediaMode::None),
        Err(Error::DestinationNotEmpty)
    ));
}

#[test]
fn csp_com_hash_dos_scripts() {
    let html = with_csp(SHELL);
    assert!(html.contains("connect-src 'none'"));
    // 'self' em file:// libera a rede inteira no WebKit (testado no Safari/Playwright).
    assert!(!html.contains("'self'"), "{html}");
    assert!(html.contains("form-action 'none'"));
    // sha256 de "console.log(1)"
    assert!(html.contains("'sha256-"), "{html}");
    let head = html.find("<head>").unwrap();
    assert!(html[head..].find("Content-Security-Policy").unwrap() < html[head..].find("<script").unwrap());
}

#[test]
fn pedacos_tem_tamanho_arredondado() {
    let f = Fixture::new();
    let (alice, _) = f.chat_ids();
    let out = tempfile::tempdir().unwrap();
    f.export(out.path(), vec![("senha do export", vec![alice])], MediaMode::None).unwrap();
    let slots = unlock(out.path(), "senha do export");
    let js = fs::read_to_string(out.path().join(format!("data/{}.js", slots[0].chats[0].chunks[0]))).unwrap();
    let blob = format::unb64(js.split('"').nth(3).unwrap()).unwrap();
    assert_eq!((blob.len() - 12 - 16) % format::PAD, 0);
}

/// Gera pacotes reais para o teste no navegador (Playwright):
/// `WA_EXPORT_OUT=<dir> WA_EXPORT_SHELL=<index.html> WA_EXPORT_IMAGE=<png> cargo test -p wa-export -- --ignored gerar`
#[test]
#[ignore]
fn gerar_pacotes_para_o_navegador() {
    let (Ok(out), Ok(shell), Ok(image)) =
        (std::env::var("WA_EXPORT_OUT"), std::env::var("WA_EXPORT_SHELL"), std::env::var("WA_EXPORT_IMAGE"))
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
