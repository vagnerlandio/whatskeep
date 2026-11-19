//! SQLite writes temporary files (sorts, indexes, FTS) to TMPDIR when it
//! does not use memory. With `temp_store = MEMORY`, nothing from the decrypted
//! history can end up there. A test file of its own: it changes the process's
//! environment variables.

use std::fs;

use rusqlite::Connection;
use wa_import::SourceApp;
use wa_vault::{KdfParams, Vault};

#[test]
fn import_writes_no_temp_files() {
    let tmp = tempfile::tempdir().unwrap();
    let vault_dir = tempfile::tempdir().unwrap();
    // SAFETY: the only test in this binary; nothing else reads the environment in parallel.
    unsafe {
        std::env::set_var("TMPDIR", tmp.path());
        std::env::set_var("SQLITE_TMPDIR", tmp.path());
    }

    let src = Connection::open_in_memory().unwrap();
    src.execute_batch(include_str!("../../wa-import/tests/fixtures/modern.sql")).unwrap();
    // Enough volume for SQLite to need temporary space (sorting for the
    // chronological import, rebuilding the FTS).
    src.execute_batch(
        "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 60000)
         INSERT INTO message (chat_row_id, from_me, key_id, sender_jid_row_id, status, timestamp, message_type, text_data)
         SELECT 1, i % 2, 'T' || i, 0, 13, 900000000 - i, 0,
                'secret test phrase number ' || i || ' ' || hex(randomblob(40))
         FROM n;",
    )
    .unwrap();

    let vault = Vault::create(vault_dir.path(), "strong password 123", KdfParams::MINIMUM).unwrap();
    let store = vault.store(SourceApp::WhatsApp).unwrap();
    store.import(&src).unwrap();
    let conn = store.open_read_only().unwrap().unwrap();
    // The real guarantee: on Unix SQLite deletes the temp file right after
    // opening it, so an empty folder alone proves nothing. 2 = MEMORY.
    let temp_store: i64 = conn.query_row("PRAGMA temp_store", [], |r| r.get(0)).unwrap();
    assert_eq!(temp_store, 2);
    let rw = store.open().unwrap().unwrap();
    assert_eq!(rw.query_row("PRAGMA temp_store", [], |r| r.get::<_, i64>(0)).unwrap(), 2);
    assert!(!wa_vault::query::search(&conn, "secret", None, None, 50).unwrap().is_empty());
    let w = vault.media(SourceApp::WhatsApp).unwrap().writer().unwrap();
    w.put(&b"secret media content".repeat(100_000)[..], None, None, Some("Media/x.jpg")).unwrap();

    let leftovers: Vec<_> = fs::read_dir(tmp.path()).unwrap().flatten().map(|e| e.file_name()).collect();
    assert!(leftovers.is_empty(), "temporary files: {leftovers:?}");
}
