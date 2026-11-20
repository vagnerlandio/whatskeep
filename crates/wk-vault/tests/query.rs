use rusqlite::Connection;
use wk_import::SourceApp;
use wk_vault::query::{self, Anchor, Cursor, Direction, Message};
use wk_vault::{KdfParams, Vault};

const FIXTURE: &str = include_str!("../../wk-import/tests/fixtures/modern.sql");

/// Temporary vault with the fixture imported; returns the read-only connection.
fn store_with(extra_sql: &str) -> (tempfile::TempDir, Connection) {
    let dir = tempfile::tempdir().unwrap();
    let vault = Vault::create(dir.path(), "strong password 123", KdfParams::MINIMUM).unwrap();
    let src = Connection::open_in_memory().unwrap();
    src.execute_batch(FIXTURE).unwrap();
    src.execute_batch(extra_sql).unwrap();
    let store = vault.store(SourceApp::WhatsApp).unwrap();
    store.import(&src).unwrap();
    let conn = store.open_read_only().unwrap().unwrap();
    (dir, conn)
}

fn fixture() -> (tempfile::TempDir, Connection) {
    store_with("")
}

fn alice_and_group(conn: &Connection) -> (i64, i64) {
    let chats = query::chats(conn).unwrap();
    let alice = chats.iter().find(|c| c.kind == "direct").unwrap().id;
    let group = chats.iter().find(|c| c.kind == "group").unwrap().id;
    (alice, group)
}

fn by_time(page: &[Message], sent_at: i64) -> &Message {
    page.iter().find(|m| m.sent_at == sent_at).unwrap()
}

fn times(page: &[Message]) -> Vec<i64> {
    page.iter().map(|m| m.sent_at).collect()
}

#[test]
fn chat_list() {
    let (_d, conn) = fixture();
    let chats = query::chats(&conn).unwrap();
    // The status is not imported and the orphan chat does not exist.
    assert_eq!(chats.len(), 2);

    let alice = &chats[0];
    assert_eq!(alice.kind, "direct");
    assert_eq!(alice.phone.as_deref(), Some("5511911111111"));
    assert_eq!(alice.title, None);
    assert_eq!(alice.message_count, 9);
    assert_eq!(alice.last_message_at, Some(14000));
    let last = alice.last.as_ref().unwrap();
    assert_eq!((last.kind.as_str(), last.text.as_deref()), ("unknown", Some("tipo novo")));

    let group = &chats[1];
    assert_eq!(group.kind, "group");
    assert_eq!(group.title.as_deref(), Some("Família"));
    assert!(group.archived);
    assert_eq!(group.last.as_ref().unwrap().kind, "system");
}

#[test]
fn chats_with_only_system_notices_are_not_listed() {
    // Carol's chat only has an automatic notice, like the thousands WhatsApp
    // keeps for contacts it touched. The group ends with a notice but has
    // real messages, so it stays.
    let (_d, conn) = store_with(
        "INSERT INTO chat (_id, jid_row_id, hidden, subject, archived) VALUES (9, 7, 0, NULL, 0);
         INSERT INTO message (_id, chat_row_id, from_me, key_id, sender_jid_row_id, status, timestamp, message_type, text_data, starred)
             VALUES (90, 9, 0, 'C1', 0, 6, 50000, 7, NULL, 0);
         INSERT INTO message_system VALUES (90, 67);",
    );
    let chats = query::chats(&conn).unwrap();
    assert_eq!(chats.len(), 2, "{:?}", chats.iter().map(|c| &c.jid).collect::<Vec<_>>());
    assert!(chats.iter().all(|c| c.jid != "55555@lid"));
    assert!(chats.iter().any(|c| c.kind == "group" && c.last.as_ref().unwrap().kind == "system"));
}

#[test]
fn pagination_backward_and_forward() {
    let (_d, conn) = fixture();
    let (alice, _) = alice_and_group(&conn);

    let last = query::messages(&conn, alice, Direction::Before, None, 4).unwrap();
    assert_eq!(times(&last), [6000, 10000, 11000, 14000]);
    let cursor = Cursor { sent_at: last[0].sent_at, id: last[0].id };
    let older = query::messages(&conn, alice, Direction::Before, Some(cursor), 4).unwrap();
    assert_eq!(times(&older), [2000, 3000, 4000, 5000]);
    let cursor = Cursor { sent_at: older[0].sent_at, id: older[0].id };
    let oldest = query::messages(&conn, alice, Direction::Before, Some(cursor), 4).unwrap();
    assert_eq!(times(&oldest), [1000]);

    let first = query::messages(&conn, alice, Direction::After, None, 3).unwrap();
    assert_eq!(times(&first), [1000, 2000, 3000]);
    let cursor = Cursor { sent_at: 3000, id: first[2].id };
    let next = query::messages(&conn, alice, Direction::After, Some(cursor), 2).unwrap();
    assert_eq!(times(&next), [4000, 5000]);
}

#[test]
fn message_content() {
    let (_d, conn) = fixture();
    let (alice, group) = alice_and_group(&conn);
    let page = query::messages(&conn, alice, Direction::After, None, 100).unwrap();

    let first = by_time(&page, 1000);
    assert_eq!(first.reactions.len(), 1);
    assert_eq!((first.reactions[0].emoji.as_str(), first.reactions[0].mine), ("❤️", true));

    let reply = by_time(&page, 2000);
    assert!(reply.from_me && reply.starred);
    let quoted = reply.quoted.as_ref().unwrap();
    assert_eq!(quoted.message_id, Some(first.id));
    assert!(quoted.text.as_deref().unwrap().contains("café"));

    let photo = by_time(&page, 3000);
    assert_eq!(photo.kind, "image");
    assert_eq!(photo.text.as_deref(), Some("foto da praia"));
    let media = photo.media.as_ref().unwrap();
    assert_eq!((media.width, media.height, media.has_thumbnail), (Some(1600), Some(1200), true));
    let thumb = query::thumbnail(&conn, photo.id).unwrap().unwrap();
    assert_eq!(&thumb[..2], [0xFF, 0xD8]);
    assert_eq!(query::thumbnail(&conn, first.id).unwrap(), None);

    let voice = by_time(&page, 4000);
    assert_eq!(voice.kind, "voice");
    let media = voice.media.as_ref().unwrap();
    assert_eq!(media.transcription.as_deref(), Some("chego em dez minutos"));
    assert_eq!((media.width, media.has_thumbnail), (None, false));

    for t in [5000, 6000] {
        let m = by_time(&page, t);
        assert_eq!((m.kind.as_str(), m.text.as_deref()), ("deleted", None));
    }

    let place = by_time(&page, 10000).location.as_ref().unwrap();
    assert_eq!(place.name.as_deref(), Some("Av. Paulista"));
    assert!((place.latitude + 23.5614).abs() < 1e-9);

    let contact = by_time(&page, 11000);
    assert_eq!(contact.contacts.len(), 1);
    assert_eq!(contact.contacts[0].name.as_deref(), Some("Zé"));
    assert_eq!(contact.contacts[0].phones, ["+55 11 90000-0000"]);

    let group_page = query::messages(&conn, group, Direction::After, None, 100).unwrap();
    let hello = by_time(&group_page, 7000);
    // LID converted to the phone number.
    assert_eq!(hello.sender.as_ref().unwrap().phone.as_deref(), Some("5511922222222"));
    // The empty (removed) reaction does not show up.
    assert_eq!(hello.reactions.len(), 1);
    assert_eq!((hello.reactions[0].emoji.as_str(), hello.reactions[0].count, hello.reactions[0].mine), ("👍", 1, false));
    let carol = by_time(&group_page, 8000).sender.as_ref().unwrap();
    assert_eq!((carol.jid.as_str(), carol.phone.as_deref()), ("55555@lid", None));
    let system = by_time(&group_page, 9000);
    assert_eq!((system.kind.as_str(), system.system_action), ("system", Some(4)));
}

#[test]
fn around_an_anchor() {
    let (_d, conn) = fixture();
    let (alice, group) = alice_and_group(&conn);
    let all = query::messages(&conn, alice, Direction::After, None, 100).unwrap();
    let target = by_time(&all, 5000).id;

    let around = query::messages_around(&conn, alice, Anchor::Message { id: target }, 4).unwrap();
    assert_eq!(times(&around.messages), [3000, 4000, 5000, 6000, 10000]);
    assert_eq!(around.anchor_index, Some(2));

    let at = query::messages_around(&conn, alice, Anchor::Time { at: 9500 }, 2).unwrap();
    assert_eq!(at.messages[at.anchor_index.unwrap()].sent_at, 10000);

    // After the last message: anchor on the last one.
    let end = query::messages_around(&conn, alice, Anchor::Time { at: 999_999 }, 4).unwrap();
    assert_eq!(times(&end.messages), [10000, 11000, 14000]);
    assert_eq!(end.anchor_index, Some(2));

    // Message from another chat: nothing.
    let wrong = query::messages_around(&conn, group, Anchor::Message { id: target }, 4).unwrap();
    assert!(wrong.messages.is_empty() && wrong.anchor_index.is_none());
}

#[test]
fn search() {
    let (_d, conn) = fixture();
    let (_, group) = alice_and_group(&conn);

    let hits = query::search(&conn, "CAFE", None, None, 10).unwrap();
    assert_eq!(hits.len(), 1);
    assert!(hits[0].snippet.contains("\u{1}café\u{2}"), "{:?}", hits[0].snippet);

    // Prefix, from most recent to oldest.
    let hits = query::search(&conn, "tud", None, None, 10).unwrap();
    assert_eq!(hits.iter().map(|h| h.sent_at).collect::<Vec<_>>(), [2000, 1000]);

    let page1 = query::search(&conn, "tud", None, None, 1).unwrap();
    let cursor = Cursor { sent_at: page1[0].sent_at, id: page1[0].message_id };
    let page2 = query::search(&conn, "tud", None, Some(cursor), 1).unwrap();
    assert_eq!(page2[0].sent_at, 1000);

    assert_eq!(query::search(&conn, "grupo", Some(group), None, 10).unwrap().len(), 1);
    assert!(query::search(&conn, "café", Some(group), None, 10).unwrap().is_empty());

    // Deleted messages are not found.
    assert!(query::search(&conn, "segredo", None, None, 10).unwrap().is_empty());

    // FTS5 syntax typed by the user does not break the search.
    for q in ["OR", "\"", "a AND", "NEAR(", "*", "-x", "(", ""] {
        query::search(&conn, q, None, None, 10).unwrap();
    }
}

#[test]
fn viewer_connection_is_read_only() {
    let (_d, conn) = fixture();
    assert!(conn.execute("DELETE FROM message", []).is_err());
}

#[test]
fn hostile_text_comes_back_intact() {
    // Message content is data, not markup: the query neither escapes nor alters it.
    let payload = "<img src=x onerror=alert(1)> javascript:alert(1)";
    let (_d, conn) = store_with(&format!(
        "INSERT INTO message (_id, chat_row_id, from_me, key_id, sender_jid_row_id, status, timestamp, message_type, text_data)
         VALUES (100, 1, 0, 'H1', 0, 13, 20000, 0, '{payload}');"
    ));
    let (alice, _) = alice_and_group(&conn);
    let last = query::messages(&conn, alice, Direction::Before, None, 1).unwrap();
    assert_eq!(last[0].text.as_deref(), Some(payload));
}

/// `cargo test -p wk-vault --test query -- --ignored --nocapture`
#[test]
#[ignore]
fn performance_300k() {
    let (_d, conn) = store_with(
        "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 300000)
         INSERT INTO message (chat_row_id, from_me, key_id, sender_jid_row_id, status, timestamp, message_type, text_data)
         SELECT 1 + (i % 2), i % 2, 'P' || i, CASE i % 2 WHEN 1 THEN 4 ELSE 0 END, 13, 100000 + i, 0,
                'mensagem número ' || i || ' com um texto de tamanho médio, parecido com conversas reais'
         FROM n;",
    );
    let (alice, _) = alice_and_group(&conn);
    let time = |label: &str, f: &mut dyn FnMut() -> usize| {
        let t = std::time::Instant::now();
        let n = f();
        println!("{label:<40} {:>9.2?}  ({n})", t.elapsed());
    };
    time("chat list", &mut || query::chats(&conn).unwrap().len());
    time("last 100 messages", &mut || {
        query::messages(&conn, alice, Direction::Before, None, 100).unwrap().len()
    });
    time("100 around a date in the middle", &mut || {
        query::messages_around(&conn, alice, Anchor::Time { at: 250_000 }, 100).unwrap().messages.len()
    });
    time("rare search (\"123456\")", &mut || query::search(&conn, "123456", None, None, 50).unwrap().len());
    time("common search (\"mensagem\", 50 of 300k)", &mut || {
        query::search(&conn, "mensagem", None, None, 50).unwrap().len()
    });
    time("common search within one chat", &mut || {
        query::search(&conn, "conversas reais", Some(alice), None, 50).unwrap().len()
    });
}


const CARDS: &str = include_str!("../../wk-import/tests/fixtures/modern-cards.sql");

#[test]
fn cards_reach_the_viewer() {
    let (_d, conn) = store_with(CARDS);
    let (_, group) = alice_and_group(&conn);
    let page = query::messages(&conn, group, Direction::Before, None, 50).unwrap();
    let poll = by_time(&page, 20000);
    assert_eq!(poll.kind, "poll");
    let card = poll.card.as_ref().unwrap();
    assert_eq!(card["type"], "poll");
    assert_eq!(card["options"][0]["name"], "Pizza");
    // Serialized for the UI as `card`.
    let json = serde_json::to_value(poll).unwrap();
    assert_eq!(json["card"]["options"][1]["votes"], 1);
    assert!(by_time(&page, 7000).card.is_none());
}

#[test]
fn history_without_the_card_table_still_opens() {
    // Imported before schema 2.
    let src = Connection::open_in_memory().unwrap();
    src.execute_batch(FIXTURE).unwrap();
    let mut dst = Connection::open_in_memory().unwrap();
    wk_import::import(&src, &mut dst, &wk_import::ImportOptions { source_app: SourceApp::WhatsApp }).unwrap();
    dst.execute_batch("DROP TABLE card").unwrap();
    let (alice, _) = alice_and_group(&dst);
    let page = query::messages(&dst, alice, Direction::Before, None, 50).unwrap();
    assert!(!page.is_empty());
    assert!(page.iter().all(|m| m.card.is_none()));
}
