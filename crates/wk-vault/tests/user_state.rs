use std::fs;

use rusqlite::Connection;
use wk_import::SourceApp;
use wk_vault::query;
use wk_vault::{KdfParams, Vault};

fn msgstore() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(include_str!("../../wk-import/tests/fixtures/modern.sql")).unwrap();
    conn
}

/// Chat list as the app shows it: (jid, pinned), in order.
fn listed(vault: &Vault) -> Vec<(String, bool)> {
    let conn = vault.store(SourceApp::WhatsApp).unwrap().open_read_only().unwrap().unwrap();
    let mut chats = query::chats(&conn).unwrap();
    query::apply_pins(&mut chats, &vault.user_state().unwrap().pins(SourceApp::WhatsApp).unwrap());
    chats.into_iter().map(|c| (c.jid, c.pinned)).collect()
}

#[test]
fn pinned_chats_go_first_and_survive_a_reimport() {
    let dir = tempfile::tempdir().unwrap();
    let vault = Vault::create(dir.path(), "strong password 123", KdfParams::MINIMUM).unwrap();
    let store = vault.store(SourceApp::WhatsApp).unwrap();
    store.import(&msgstore()).unwrap();
    let alice = "5511911111111@s.whatsapp.net".to_owned();
    let group = "120363000000000001@g.us".to_owned();

    // Nothing pinned: the newest chat first (Alice).
    assert_eq!(listed(&vault), [(alice.clone(), false), (group.clone(), false)]);

    let state = vault.user_state().unwrap();
    state.set_pinned(SourceApp::WhatsApp, &group, true).unwrap();
    assert_eq!(listed(&vault), [(group.clone(), true), (alice.clone(), false)]);

    // The most recently pinned goes first.
    std::thread::sleep(std::time::Duration::from_millis(5));
    state.set_pinned(SourceApp::WhatsApp, &alice, true).unwrap();
    assert_eq!(listed(&vault), [(alice.clone(), true), (group.clone(), true)]);

    // Pins belong to one app.
    assert!(state.pins(SourceApp::Business).unwrap().is_empty());

    // Re-importing the backup keeps them.
    store.import(&msgstore()).unwrap();
    assert_eq!(listed(&vault), [(alice.clone(), true), (group.clone(), true)]);

    // Unpinning; then forgetting the app (its history was deleted).
    state.set_pinned(SourceApp::WhatsApp, &alice, false).unwrap();
    assert_eq!(listed(&vault), [(group.clone(), true), (alice.clone(), false)]);
    state.forget_app(SourceApp::WhatsApp).unwrap();
    assert_eq!(listed(&vault), [(alice.clone(), false), (group, false)]);

    // Encrypted on disk.
    let raw = fs::read(dir.path().join("stores/user-state.db")).unwrap();
    assert!(!raw.windows(13).any(|w| w == b"5511911111111"));
}
