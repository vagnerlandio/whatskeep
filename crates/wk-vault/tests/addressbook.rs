use std::collections::HashMap;
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

fn names(vault: &Vault) -> HashMap<String, Option<String>> {
    let conn = vault.store(SourceApp::WhatsApp).unwrap().open_read_only().unwrap().unwrap();
    query::chats(&conn).unwrap().into_iter().map(|c| (c.jid, c.name)).collect()
}

#[test]
fn names_come_from_the_address_book_and_survive_a_reimport() {
    let dir = tempfile::tempdir().unwrap();
    let vault = Vault::create(dir.path(), "strong password 123", KdfParams::MINIMUM).unwrap();
    let store = vault.store(SourceApp::WhatsApp).unwrap();
    store.import(&msgstore()).unwrap();

    let book = vault.address_book().unwrap();
    assert!(book.info().unwrap().is_none());
    // Alice as WhatsApp has her; Bob saved without the Brazilian mobile "9"
    // (WhatsApp has 55 11 9 2222-2222).
    let numbers: HashMap<String, String> = [
        ("5511911111111".to_owned(), "Alice Souza".to_owned()),
        ("551122222222".to_owned(), "Bob Lima".to_owned()),
        ("5521900000000".to_owned(), "Not in the history".to_owned()),
    ]
    .into();
    let info = book.replace(&numbers, 3).unwrap();
    assert_eq!((info.people, info.numbers), (3, 3));
    assert_eq!(book.info().unwrap().unwrap().people, 3);
    assert_eq!(book.load().unwrap().unwrap(), numbers);

    let applied = store.apply_names(book.load().unwrap().as_ref()).unwrap();
    assert_eq!(applied.contacts, 2, "Alice and Bob");
    assert_eq!(applied.chats, 1, "Alice's chat (the group has its own title)");
    let n = names(&vault);
    assert_eq!(n["5511911111111@s.whatsapp.net"].as_deref(), Some("Alice Souza"));

    // Bob writes in the group (via his LID mapped to the phone): the sender carries the name.
    let conn = store.open_read_only().unwrap().unwrap();
    let group = query::chats(&conn).unwrap().into_iter().find(|c| c.kind == "group").unwrap();
    let page = query::messages(&conn, group.id, query::Direction::Before, None, 50).unwrap();
    assert!(page.iter().any(|m| m.sender.as_ref().and_then(|s| s.name.as_deref()) == Some("Bob Lima")));
    drop(conn);

    // Re-importing replaces the history: the names go and come back when applied.
    store.import(&msgstore()).unwrap();
    assert_eq!(names(&vault)["5511911111111@s.whatsapp.net"], None);
    store.apply_names(book.load().unwrap().as_ref()).unwrap();
    assert_eq!(names(&vault)["5511911111111@s.whatsapp.net"].as_deref(), Some("Alice Souza"));

    // The names are not on disk in plaintext.
    let raw = fs::read(dir.path().join("stores/address-book.db")).unwrap();
    assert!(!raw.windows(5).any(|w| w == b"Alice"));

    // Removing: no address book, no names.
    book.delete().unwrap();
    assert!(book.info().unwrap().is_none());
    store.apply_names(None).unwrap();
    assert_eq!(names(&vault)["5511911111111@s.whatsapp.net"], None);
    assert!(!dir.path().join("stores/address-book.db").exists());
}
