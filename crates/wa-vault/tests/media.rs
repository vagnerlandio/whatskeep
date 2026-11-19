use std::fs;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use sha2::{Digest, Sha256};
use wa_import::SourceApp;
use wa_vault::media::{SEGMENT, normalize_path};
use wa_vault::{Error, KdfParams, Put, Vault};

fn vault() -> (tempfile::TempDir, Vault) {
    let dir = tempfile::tempdir().unwrap();
    let v = Vault::create(dir.path(), "senha forte 123", KdfParams::MINIMUM).unwrap();
    (dir, v)
}

/// Conteúdo pseudoaleatório determinístico (não comprimível, sem padrões).
fn content(n: usize, seed: u8) -> Vec<u8> {
    let mut out = Vec::with_capacity(n);
    let mut x = seed as u32 | 1;
    while out.len() < n {
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        out.extend_from_slice(&x.to_le_bytes());
    }
    out.truncate(n);
    out
}

fn sha(b: &[u8]) -> String {
    B64.encode(Sha256::digest(b))
}

fn blob_files(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    fs::read_dir(dir.join("stores/whatsapp-media")).map(|d| d.flatten().map(|e| e.path()).collect()).unwrap_or_default()
}

#[test]
fn caminhos_normalizados() {
    assert_eq!(normalize_path("Media/WhatsApp Images/IMG-1.jpg").as_deref(), Some("Media/WhatsApp Images/IMG-1.jpg"));
    assert_eq!(
        normalize_path("/storage/emulated/0/WhatsApp/Media/WhatsApp Voice Notes/202601/PTT-1.opus").as_deref(),
        Some("Media/WhatsApp Voice Notes/202601/PTT-1.opus")
    );
    assert_eq!(
        normalize_path("C:\\copia\\WhatsApp Business\\Media\\WhatsApp Business Images\\x.jpg").as_deref(),
        Some("Media/WhatsApp Business Images/x.jpg")
    );
    assert_eq!(
        normalize_path("/Volumes/Media/copia/WhatsApp/Media/WhatsApp Audio/AUD-1.opus").as_deref(),
        Some("Media/WhatsApp Audio/AUD-1.opus")
    );
    assert_eq!(normalize_path("Databases/msgstore.db.crypt15"), None);
    assert_eq!(normalize_path("Media/"), None);
}

#[test]
fn ida_e_volta_em_todos_os_tamanhos() {
    let (_d, v) = vault();
    let h = v.media(SourceApp::WhatsApp).unwrap();
    assert!(h.reader().unwrap().is_none());
    let w = h.writer().unwrap();
    for (i, n) in [0, 1, 1000, SEGMENT - 1, SEGMENT, SEGMENT + 1, 3 * SEGMENT + 5].into_iter().enumerate() {
        let data = content(n, i as u8 + 1);
        let (hash, put) = w.put(&data[..], Some(&sha(&data)), Some("image/jpeg"), None).unwrap();
        assert_eq!((hash.as_str(), put), (sha(&data).as_str(), Put::Added), "tamanho {n}");

        let r = h.reader().unwrap().unwrap();
        let blob = r.find(Some(&hash), None).unwrap().unwrap();
        assert_eq!(blob.size, n as u64);
        assert_eq!(&*r.read_range(&blob, 0, u64::MAX).unwrap(), &data[..], "tamanho {n}");
        let mut copy = Vec::new();
        r.copy_to(&blob, &mut copy).unwrap();
        assert_eq!(copy, data);
        // Trechos atravessando a fronteira dos segmentos.
        if n > SEGMENT {
            let (a, b) = (SEGMENT as u64 - 10, (SEGMENT as u64 + 10).min(n as u64));
            assert_eq!(&*r.read_range(&blob, a, b).unwrap(), &data[a as usize..b as usize]);
            let tail = n as u64 - 3;
            assert_eq!(&*r.read_range(&blob, tail, n as u64 + 100).unwrap(), &data[tail as usize..]);
        }
        assert!(r.read_range(&blob, n as u64, n as u64 + 5).unwrap().is_empty());
    }
}

#[test]
fn deduplica_e_acha_pelo_caminho() {
    let (dir, v) = vault();
    let h = v.media(SourceApp::WhatsApp).unwrap();
    let w = h.writer().unwrap();
    let data = content(5000, 9);
    let (hash, put) = w.put(&data[..], None, None, Some("/sdcard/WhatsApp/Media/WhatsApp Images/A.jpg")).unwrap();
    assert_eq!(put, Put::Added);
    let (_, put) = w.put(&data[..], None, None, Some("Media/WhatsApp Images/Sent/B.jpg")).unwrap();
    assert_eq!(put, Put::AlreadyStored);
    assert_eq!(blob_files(dir.path()).len(), 1, "um blob só");

    let r = h.reader().unwrap().unwrap();
    for p in ["Media/WhatsApp Images/A.jpg", "Media/WhatsApp Images/Sent/B.jpg"] {
        assert_eq!(r.find(None, Some(p)).unwrap().unwrap().sha256, hash);
    }
    assert!(r.find(Some("outro"), Some("Media/x.jpg")).unwrap().is_none());
}

#[test]
fn hash_divergente_nao_grava_nada() {
    let (dir, v) = vault();
    let w = v.media(SourceApp::WhatsApp).unwrap().writer().unwrap();
    let err = w.put(&b"conteudo"[..], Some("hash-que-nao-confere"), None, Some("Media/x.jpg")).err().unwrap();
    assert!(matches!(err, Error::Corrupt(_)));
    assert!(blob_files(dir.path()).is_empty());
    assert!(w.find(None, Some("Media/x.jpg")).unwrap().is_none());
}

#[test]
fn adulteracao_e_detectada() {
    let (dir, v) = vault();
    let h = v.media(SourceApp::WhatsApp).unwrap();
    let w = h.writer().unwrap();
    let a = content(2 * SEGMENT + 100, 1);
    let b = content(2 * SEGMENT + 100, 2);
    let (ha, _) = w.put(&a[..], None, None, None).unwrap();
    let (hb, _) = w.put(&b[..], None, None, None).unwrap();
    let r = h.reader().unwrap().unwrap();
    let (ba, bb) = (r.find(Some(&ha), None).unwrap().unwrap(), r.find(Some(&hb), None).unwrap().unwrap());
    let path = |id: &str| dir.path().join("stores/whatsapp-media").join(id);
    let original = fs::read(path(&ba.id)).unwrap();

    // Um bit trocado no meio.
    let mut bad = original.clone();
    bad[SEGMENT + 500] ^= 1;
    fs::write(path(&ba.id), &bad).unwrap();
    assert!(matches!(r.read_range(&ba, SEGMENT as u64, SEGMENT as u64 + 10), Err(Error::Corrupt(_))));
    assert!(r.read_range(&ba, 0, 10).is_ok(), "segmentos intactos continuam legíveis");

    // Último segmento cortado fora (truncamento na fronteira).
    fs::write(path(&ba.id), &original[..12 + 2 * (SEGMENT + 16)]).unwrap();
    assert!(r.read_range(&ba, 0, u64::MAX).is_err());

    // Troca de um blob por outro (mesma chave, id diferente no AAD).
    fs::write(path(&ba.id), fs::read(path(&bb.id)).unwrap()).unwrap();
    assert!(matches!(r.read_range(&ba, 0, 10), Err(Error::Corrupt(_))));

    fs::write(path(&ba.id), &original).unwrap();
    assert_eq!(&*r.read_range(&ba, 0, u64::MAX).unwrap(), &a[..]);
}

#[test]
fn nada_em_claro_e_chaves_por_app() {
    let (dir, v) = vault();
    let marker = b"FOTO-SECRETA-0123456789".repeat(50);
    let w = v.media(SourceApp::WhatsApp).unwrap().writer().unwrap();
    w.put(&marker[..], None, Some("image/jpeg"), Some("Media/WhatsApp Images/IMG-SEGREDO.jpg")).unwrap();
    drop(w);
    for entry in walk(dir.path()) {
        let bytes = fs::read(&entry).unwrap();
        for needle in [&b"FOTO-SECRETA"[..], b"IMG-SEGREDO", b"image/jpeg", b"SQLite format 3"] {
            assert!(!bytes.windows(needle.len()).any(|w| w == needle), "{entry:?}");
        }
    }
    // O índice do Business não abre o do WhatsApp.
    let stores = dir.path().join("stores");
    fs::copy(stores.join("whatsapp-media.db"), stores.join("business-media.db")).unwrap();
    assert!(matches!(v.media(SourceApp::Business).unwrap().reader(), Err(Error::StoreUnreadable)));
}

fn walk(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    for e in fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() { out.extend(walk(&p)) } else { out.push(p) }
    }
    out
}

#[test]
fn mensagem_acha_a_midia_importada() {
    use rusqlite::Connection;
    use wa_vault::query::{self, Direction};

    let (_d, v) = vault();
    let src = Connection::open_in_memory().unwrap();
    src.execute_batch(include_str!("../../wa-import/tests/fixtures/modern.sql")).unwrap();
    let store = v.store(SourceApp::WhatsApp).unwrap();
    store.import(&src).unwrap();
    let conn = store.open_read_only().unwrap().unwrap();

    // O que o msgstore espera (a foto e o áudio do fixture).
    let refs = query::media_refs(&conn).unwrap();
    let photo_ref = refs.iter().find(|r| r.kind == "image").unwrap();
    assert_eq!(photo_ref.file_path.as_deref(), Some("Media/WhatsApp Images/IMG-0001.jpg"));

    // Arquivo copiado do celular para uma pasta qualquer.
    let local = "/Users/fulano/Desktop/WhatsApp/Media/WhatsApp Images/IMG-0001.jpg";
    let photo = content(70_000, 42);
    let w = v.media(SourceApp::WhatsApp).unwrap().writer().unwrap();
    w.put(&photo[..], None, photo_ref.mime.as_deref(), Some(local)).unwrap();

    let reader = v.media(SourceApp::WhatsApp).unwrap().reader().unwrap().unwrap();
    let chats = query::chats(&conn).unwrap();
    let alice = chats.iter().find(|c| c.kind == "direct").unwrap().id;
    let page = query::messages(&conn, alice, Direction::After, None, 100).unwrap();
    let mut found = 0;
    for m in &page {
        let Some(media) = &m.media else { continue };
        let blob = reader.find(media.file_hash.as_deref(), media.file_path.as_deref()).unwrap();
        if m.kind == "image" {
            let blob = blob.expect("a foto deve ser achada pelo caminho");
            assert_eq!(&*reader.read_range(&blob, 0, u64::MAX).unwrap(), &photo[..]);
            found += 1;
        } else {
            assert!(blob.is_none(), "o áudio não foi importado");
        }
    }
    assert_eq!(found, 1);
    // A referência por id (usada pelo protocolo wamedia://) aponta para o mesmo arquivo.
    let r = query::media_ref(&conn, page.iter().find(|m| m.kind == "image").unwrap().id).unwrap().unwrap();
    assert!(reader.find(r.file_hash.as_deref(), r.file_path.as_deref()).unwrap().is_some());
}
