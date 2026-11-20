use std::fs;
use std::path::Path;

use rusqlite::Connection;
use wk_import::SourceApp;
use wk_vault::{Error, KdfParams, Vault};

const FAST: KdfParams = KdfParams::MINIMUM;
const PW: &str = "strong password 123";

fn msgstore() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(include_str!("../../wk-import/tests/fixtures/modern.sql")).unwrap();
    conn
}

fn all_bytes(dir: &Path) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    for entry in fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(all_bytes(&path));
        } else {
            out.push((path.display().to_string(), fs::read(&path).unwrap()));
        }
    }
    out
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

#[test]
fn creates_unlocks_and_rejects_wrong_password() {
    let dir = tempfile::tempdir().unwrap();
    assert!(!Vault::exists(dir.path()));
    drop(Vault::create(dir.path(), PW, FAST).unwrap());
    assert!(Vault::exists(dir.path()));

    Vault::unlock(dir.path(), PW).unwrap();
    assert!(matches!(Vault::unlock(dir.path(), "wrong password!"), Err(Error::WrongPassword)));
    assert!(matches!(Vault::create(dir.path(), PW, FAST), Err(Error::AlreadyExists)));
}

#[test]
fn short_password_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    assert!(matches!(Vault::create(dir.path(), "1234567", FAST), Err(Error::WeakPassword)));
    // Counts characters, not bytes.
    assert!(matches!(Vault::create(dir.path(), "çãéíóúâ", FAST), Err(Error::WeakPassword)));
    Vault::create(dir.path(), "çãéíóúâê", FAST).unwrap();
}

#[test]
fn missing_vault_returns_not_found() {
    let dir = tempfile::tempdir().unwrap();
    assert!(matches!(Vault::unlock(dir.path(), PW), Err(Error::NotFound)));
}

#[test]
fn tampered_header_does_not_open() {
    let dir = tempfile::tempdir().unwrap();
    drop(Vault::create(dir.path(), PW, KdfParams { m_kib: 16 * 1024, t: 2, p: 1 }).unwrap());
    let path = dir.path().join("vault.json");
    let original = fs::read_to_string(&path).unwrap();

    // Lowering the KDF cost changes the AAD: the GCM tag does not match.
    fs::write(&path, original.replace("\"t\": 2", "\"t\": 1")).unwrap();
    assert!(matches!(Vault::unlock(dir.path(), PW), Err(Error::WrongPassword)));

    // Absurd parameters are rejected before running Argon2id.
    fs::write(&path, original.replace("\"m_kib\": 16384", "\"m_kib\": 99999999")).unwrap();
    assert!(matches!(Vault::unlock(dir.path(), PW), Err(Error::Corrupt(_))));

    fs::write(&path, "not json").unwrap();
    assert!(matches!(Vault::unlock(dir.path(), PW), Err(Error::Corrupt(_))));

    fs::write(&path, &original).unwrap();
    Vault::unlock(dir.path(), PW).unwrap();
}

#[test]
fn imports_and_reads_after_lock_and_unlock() {
    let dir = tempfile::tempdir().unwrap();
    let vault = Vault::create(dir.path(), PW, FAST).unwrap();
    assert!(vault.stores().unwrap().is_empty());

    let store = vault.store(SourceApp::WhatsApp).unwrap();
    assert!(store.open().unwrap().is_none());
    let report = store.import(&msgstore()).unwrap();
    assert!(report.messages > 0);
    drop(store);
    drop(vault); // locks

    let vault = Vault::unlock(dir.path(), PW).unwrap();
    let stores = vault.stores().unwrap();
    assert_eq!(stores.len(), 1);
    assert_eq!(stores[0].source_app, SourceApp::WhatsApp);
    assert_eq!(stores[0].messages, report.messages);
    assert_eq!(stores[0].chats, report.chats);
    assert_eq!(stores[0].source_schema, "modern");
    assert!(stores[0].imported_at_ms > 1_700_000_000_000);

    let conn = vault.store(SourceApp::WhatsApp).unwrap().open().unwrap().unwrap();
    let hits: i64 = conn
        .query_row("SELECT count(*) FROM message_fts WHERE message_fts MATCH 'cafe'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(hits, 1);
}

#[test]
fn no_plaintext_on_disk() {
    let dir = tempfile::tempdir().unwrap();
    let vault = Vault::create(dir.path(), PW, FAST).unwrap();
    vault.store(SourceApp::WhatsApp).unwrap().import(&msgstore()).unwrap();
    vault.store(SourceApp::Business).unwrap().import(&msgstore()).unwrap();
    drop(vault);

    let files = all_bytes(dir.path());
    assert!(files.iter().any(|(p, _)| p.ends_with("whatsapp.db")));
    assert!(files.iter().any(|(p, _)| p.ends_with("business.db")));
    for (path, bytes) in &files {
        for needle in [
            "Vamos tomar um caf".as_bytes(),
            b"5511911111111",
            b"s.whatsapp.net",
            b"SQLite format 3",
            b"CREATE TABLE",
            PW.as_bytes(),
        ] {
            assert!(!contains(bytes, needle), "{path} contains {:?}", String::from_utf8_lossy(needle));
        }
    }
}

#[cfg(unix)]
#[test]
fn restricted_permissions() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("vault");
    let vault = Vault::create(&root, PW, FAST).unwrap();
    vault.store(SourceApp::WhatsApp).unwrap().import(&msgstore()).unwrap();

    let mode = |p: &Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(&root), 0o700);
    assert_eq!(mode(&root.join("stores")), 0o700);
    assert_eq!(mode(&root.join("vault.json")), 0o600);
    assert_eq!(mode(&root.join("stores/whatsapp.db")), 0o600);
    assert!(root.join(".metadata_never_index").is_file());
}

#[test]
fn databases_have_different_keys() {
    let dir = tempfile::tempdir().unwrap();
    let vault = Vault::create(dir.path(), PW, FAST).unwrap();
    vault.store(SourceApp::WhatsApp).unwrap().import(&msgstore()).unwrap();

    // The Business database does not open with the WhatsApp key.
    let stores = dir.path().join("stores");
    fs::copy(stores.join("whatsapp.db"), stores.join("business.db")).unwrap();
    assert!(matches!(
        vault.store(SourceApp::Business).unwrap().open(),
        Err(Error::StoreUnreadable)
    ));

    // Nor in another vault with the same password (different DEK and id).
    let other = tempfile::tempdir().unwrap();
    let vault2 = Vault::create(other.path(), PW, FAST).unwrap();
    fs::copy(stores.join("whatsapp.db"), other.path().join("stores/whatsapp.db")).unwrap();
    assert!(matches!(
        vault2.store(SourceApp::WhatsApp).unwrap().open(),
        Err(Error::StoreUnreadable)
    ));
}

#[test]
fn reimport_replaces() {
    let dir = tempfile::tempdir().unwrap();
    let vault = Vault::create(dir.path(), PW, FAST).unwrap();
    let store = vault.store(SourceApp::WhatsApp).unwrap();
    let first = store.import(&msgstore()).unwrap();
    let second = store.import(&msgstore()).unwrap();
    assert_eq!(first, second);
    assert_eq!(vault.stores().unwrap()[0].messages, second.messages);
}

#[test]
fn failed_import_keeps_previous_database() {
    let dir = tempfile::tempdir().unwrap();
    let vault = Vault::create(dir.path(), PW, FAST).unwrap();
    let store = vault.store(SourceApp::WhatsApp).unwrap();
    let report = store.import(&msgstore()).unwrap();

    let not_msgstore = Connection::open_in_memory().unwrap();
    not_msgstore.execute_batch("CREATE TABLE foo (x)").unwrap();
    assert!(matches!(store.import(&not_msgstore), Err(Error::Import(wk_import::Error::NotMsgstore))));

    assert_eq!(vault.stores().unwrap()[0].messages, report.messages);
    let leftovers: Vec<_> = fs::read_dir(dir.path().join("stores"))
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(leftovers, ["whatsapp.db"]);
}

#[test]
fn stale_temp_files_are_cleaned_on_unlock() {
    let dir = tempfile::tempdir().unwrap();
    drop(Vault::create(dir.path(), PW, FAST).unwrap());
    let stale = dir.path().join("stores/whatsapp.db.tmp-0011aabbccdd");
    fs::write(&stale, b"encrypted garbage").unwrap();
    Vault::unlock(dir.path(), PW).unwrap();
    assert!(!stale.exists());
}

#[test]
fn password_change() {
    let dir = tempfile::tempdir().unwrap();
    let mut vault = Vault::create(dir.path(), PW, FAST).unwrap();
    vault.store(SourceApp::WhatsApp).unwrap().import(&msgstore()).unwrap();

    assert!(matches!(
        vault.change_password("random guess", "new password 456", FAST),
        Err(Error::WrongPassword)
    ));
    assert!(matches!(vault.change_password(PW, "short", FAST), Err(Error::WeakPassword)));
    vault.change_password(PW, "new password 456", FAST).unwrap();
    drop(vault);

    assert!(matches!(Vault::unlock(dir.path(), PW), Err(Error::WrongPassword)));
    let vault = Vault::unlock(dir.path(), "new password 456").unwrap();
    // Same DEK: the databases keep opening without re-encryption.
    assert_eq!(vault.stores().unwrap().len(), 1);
}

#[test]
fn debug_does_not_leak_key() {
    let dir = tempfile::tempdir().unwrap();
    let vault = Vault::create(dir.path(), PW, FAST).unwrap();
    let dbg = format!("{vault:?} {:?}", vault.store(SourceApp::WhatsApp).unwrap());
    assert!(dbg.contains("<redacted>"));
    assert!(!dbg.contains("key: ["));
}

/// Measures Argon2id with the default parameters:
/// `cargo test -p wk-vault --release -- --ignored --nocapture`
#[test]
#[ignore]
fn default_kdf_timing() {
    let dir = tempfile::tempdir().unwrap();
    drop(Vault::create(dir.path(), PW, KdfParams::DEFAULT).unwrap());
    let t = std::time::Instant::now();
    Vault::unlock(dir.path(), PW).unwrap();
    println!("unlock with KdfParams::DEFAULT: {:?}", t.elapsed());
}

#[cfg(target_os = "macos")]
#[test]
fn excluded_from_time_machine() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("vault");
    drop(Vault::create(&root, PW, FAST).unwrap());
    let out = std::process::Command::new("tmutil").arg("isexcluded").arg(&root).output().unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("[Excluded]"), "{text}");
}

/// A vault from the development format 1 (`wa-vault` labels) gets a clear
/// error instead of looking like a wrong password.
#[test]
fn outdated_format_has_a_clear_error() {
    let dir = tempfile::tempdir().unwrap();
    drop(Vault::create(dir.path(), PW, FAST).unwrap());
    let path = dir.path().join("vault.json");
    let header = fs::read_to_string(&path).unwrap();
    assert!(header.contains("\"format\": 2"), "{header}");
    fs::write(&path, header.replace("\"format\": 2", "\"format\": 1")).unwrap();
    assert!(matches!(Vault::unlock(dir.path(), PW), Err(Error::OutdatedFormat)));
}

#[test]
fn deletes_one_history_and_keeps_the_other() {
    let dir = tempfile::tempdir().unwrap();
    let vault = Vault::create(dir.path(), PW, FAST).unwrap();
    for app in SourceApp::ALL {
        vault.store(app).unwrap().import(&msgstore()).unwrap();
        let writer = vault.media(app).unwrap().writer().unwrap();
        writer.put(&b"photo bytes"[..], None, Some("image/jpeg"), Some("Media/WhatsApp Images/IMG-1.jpg")).unwrap();
    }
    assert_eq!(vault.stores().unwrap().len(), 2);

    vault.delete_store(SourceApp::Business).unwrap();
    let left: Vec<SourceApp> = vault.stores().unwrap().into_iter().map(|s| s.source_app).collect();
    assert_eq!(left, [SourceApp::WhatsApp]);
    assert!(!vault.media(SourceApp::Business).unwrap().exists());
    assert!(vault.media(SourceApp::WhatsApp).unwrap().exists());
    let names: Vec<String> =
        fs::read_dir(dir.path().join("stores")).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    assert!(names.iter().all(|n| !n.starts_with("business")), "{names:?}");
    // Deleting what is not there is fine.
    vault.delete_store(SourceApp::Business).unwrap();
}

#[test]
fn destroys_the_vault_without_the_password() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("vault");
    let vault = Vault::create(&root, PW, FAST).unwrap();
    vault.store(SourceApp::WhatsApp).unwrap().import(&msgstore()).unwrap();
    drop(vault); // locked: the password is not needed to delete it
    assert!(Vault::disk_usage(&root) > 0);

    Vault::destroy(&root).unwrap();
    assert!(!root.exists());
    assert!(!Vault::exists(&root));
    assert_eq!(Vault::disk_usage(&root), 0);
    // A new vault can be created in the same place.
    Vault::create(&root, "another password 456", FAST).unwrap();
    // And destroying a folder that is already gone is fine.
    Vault::destroy(&dir.path().join("missing")).unwrap();
}
