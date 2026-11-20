use std::fs;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use sha2::{Digest, Sha256};
use wk_import::SourceApp;
use wk_vault::media::{SEGMENT, normalize_path};
use wk_vault::{Error, KdfParams, Put, Vault};

fn vault() -> (tempfile::TempDir, Vault) {
    let dir = tempfile::tempdir().unwrap();
    let v = Vault::create(dir.path(), "strong password 123", KdfParams::MINIMUM).unwrap();
    (dir, v)
}

/// Deterministic pseudo-random content (incompressible, no patterns).
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
fn normalized_paths() {
    assert_eq!(normalize_path("Media/WhatsApp Images/IMG-1.jpg").as_deref(), Some("Media/WhatsApp Images/IMG-1.jpg"));
    assert_eq!(
        normalize_path("/storage/emulated/0/WhatsApp/Media/WhatsApp Voice Notes/202601/PTT-1.opus").as_deref(),
        Some("Media/WhatsApp Voice Notes/202601/PTT-1.opus")
    );
    assert_eq!(
        normalize_path("C:\\copy\\WhatsApp Business\\Media\\WhatsApp Business Images\\x.jpg").as_deref(),
        Some("Media/WhatsApp Business Images/x.jpg")
    );
    assert_eq!(
        normalize_path("/Volumes/Media/copy/WhatsApp/Media/WhatsApp Audio/AUD-1.opus").as_deref(),
        Some("Media/WhatsApp Audio/AUD-1.opus")
    );
    assert_eq!(normalize_path("Databases/msgstore.db.crypt15"), None);
    assert_eq!(normalize_path("Media/"), None);
}

#[test]
fn round_trip_at_all_sizes() {
    let (_d, v) = vault();
    let h = v.media(SourceApp::WhatsApp).unwrap();
    assert!(h.reader().unwrap().is_none());
    let w = h.writer().unwrap();
    for (i, n) in [0, 1, 1000, SEGMENT - 1, SEGMENT, SEGMENT + 1, 3 * SEGMENT + 5].into_iter().enumerate() {
        let data = content(n, i as u8 + 1);
        let (hash, put) = w.put(&data[..], Some(&sha(&data)), Some("image/jpeg"), None).unwrap();
        assert_eq!((hash.as_str(), put), (sha(&data).as_str(), Put::Added), "size {n}");

        let r = h.reader().unwrap().unwrap();
        let blob = r.find(Some(&hash), None).unwrap().unwrap();
        assert_eq!(blob.size, n as u64);
        assert_eq!(&*r.read_range(&blob, 0, u64::MAX).unwrap(), &data[..], "size {n}");
        let mut copy = Vec::new();
        r.copy_to(&blob, &mut copy).unwrap();
        assert_eq!(copy, data);
        // Ranges crossing the segment boundary.
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
fn deduplicates_and_finds_by_path() {
    let (dir, v) = vault();
    let h = v.media(SourceApp::WhatsApp).unwrap();
    let w = h.writer().unwrap();
    let data = content(5000, 9);
    let (hash, put) = w.put(&data[..], None, None, Some("/sdcard/WhatsApp/Media/WhatsApp Images/A.jpg")).unwrap();
    assert_eq!(put, Put::Added);
    let (_, put) = w.put(&data[..], None, None, Some("Media/WhatsApp Images/Sent/B.jpg")).unwrap();
    assert_eq!(put, Put::AlreadyStored);
    assert_eq!(blob_files(dir.path()).len(), 1, "a single blob");

    let r = h.reader().unwrap().unwrap();
    for p in ["Media/WhatsApp Images/A.jpg", "Media/WhatsApp Images/Sent/B.jpg"] {
        assert_eq!(r.find(None, Some(p)).unwrap().unwrap().sha256, hash);
    }
    assert!(r.find(Some("other"), Some("Media/x.jpg")).unwrap().is_none());
}

#[test]
fn mismatching_hash_writes_nothing() {
    let (dir, v) = vault();
    let w = v.media(SourceApp::WhatsApp).unwrap().writer().unwrap();
    let err = w.put(&b"content"[..], Some("mismatching-hash"), None, Some("Media/x.jpg")).err().unwrap();
    assert!(matches!(err, Error::Corrupt(_)));
    assert!(blob_files(dir.path()).is_empty());
    assert!(w.find(None, Some("Media/x.jpg")).unwrap().is_none());
}

#[test]
fn tampering_is_detected() {
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

    // One bit flipped in the middle.
    let mut bad = original.clone();
    bad[SEGMENT + 500] ^= 1;
    fs::write(path(&ba.id), &bad).unwrap();
    assert!(matches!(r.read_range(&ba, SEGMENT as u64, SEGMENT as u64 + 10), Err(Error::Corrupt(_))));
    assert!(r.read_range(&ba, 0, 10).is_ok(), "intact segments remain readable");

    // Last segment cut off (truncation at the boundary).
    fs::write(path(&ba.id), &original[..12 + 2 * (SEGMENT + 16)]).unwrap();
    assert!(r.read_range(&ba, 0, u64::MAX).is_err());

    // Swapping one blob for another (same key, different id in the AAD).
    fs::write(path(&ba.id), fs::read(path(&bb.id)).unwrap()).unwrap();
    assert!(matches!(r.read_range(&ba, 0, 10), Err(Error::Corrupt(_))));

    fs::write(path(&ba.id), &original).unwrap();
    assert_eq!(&*r.read_range(&ba, 0, u64::MAX).unwrap(), &a[..]);
}

#[test]
fn no_plaintext_and_per_app_keys() {
    let (dir, v) = vault();
    let marker = b"SECRET-PHOTO-0123456789".repeat(50);
    let w = v.media(SourceApp::WhatsApp).unwrap().writer().unwrap();
    w.put(&marker[..], None, Some("image/jpeg"), Some("Media/WhatsApp Images/IMG-SECRET.jpg")).unwrap();
    drop(w);
    for entry in walk(dir.path()) {
        let bytes = fs::read(&entry).unwrap();
        for needle in [&b"SECRET-PHOTO"[..], b"IMG-SECRET", b"image/jpeg", b"SQLite format 3"] {
            assert!(!bytes.windows(needle.len()).any(|w| w == needle), "{entry:?}");
        }
    }
    // The Business index key does not open the WhatsApp one.
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
fn message_finds_imported_media() {
    use rusqlite::Connection;
    use wk_vault::query::{self, Direction};

    let (_d, v) = vault();
    let src = Connection::open_in_memory().unwrap();
    src.execute_batch(include_str!("../../wk-import/tests/fixtures/modern.sql")).unwrap();
    let store = v.store(SourceApp::WhatsApp).unwrap();
    store.import(&src).unwrap();
    let conn = store.open_read_only().unwrap().unwrap();

    // What the msgstore expects (the fixture's photo and audio).
    let refs = query::media_refs(&conn).unwrap();
    let photo_ref = refs.iter().find(|r| r.kind == "image").unwrap();
    assert_eq!(photo_ref.file_path.as_deref(), Some("Media/WhatsApp Images/IMG-0001.jpg"));

    // File copied from the phone into some arbitrary folder.
    let local = "/Users/someone/Desktop/WhatsApp/Media/WhatsApp Images/IMG-0001.jpg";
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
            let blob = blob.expect("the photo must be found by path");
            assert_eq!(&*reader.read_range(&blob, 0, u64::MAX).unwrap(), &photo[..]);
            found += 1;
        } else {
            assert!(blob.is_none(), "the audio was not imported");
        }
    }
    assert_eq!(found, 1);
    // The reference by id (used by the wkmedia:// protocol) points to the same file.
    let r = query::media_ref(&conn, page.iter().find(|m| m.kind == "image").unwrap().id).unwrap().unwrap();
    assert!(reader.find(r.file_hash.as_deref(), r.file_path.as_deref()).unwrap().is_some());
}
