use std::collections::BTreeMap;

use rusqlite::{Connection, OptionalExtension, MAIN_DB};
use wa_import::{import, open_msgstore, Error, ImportOptions, ImportReport, SourceApp, SourceSchema};

fn fixture_sql(name: &str) -> String {
    std::fs::read_to_string(format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

fn source(sql_file: &str) -> Connection {
    let c = Connection::open_in_memory().unwrap();
    c.execute_batch(&fixture_sql(sql_file)).unwrap();
    c
}

fn opts() -> ImportOptions {
    ImportOptions {
        source_app: SourceApp::WhatsApp,
    }
}

fn run(src: &Connection) -> (Connection, ImportReport) {
    let mut dst = Connection::open_in_memory().unwrap();
    let report = import(src, &mut dst, &opts()).unwrap();
    (dst, report)
}

/// Busca uma mensagem pelo key_id original.
fn msg(dst: &Connection, key: &str) -> MsgRow {
    dst.query_row(
        "SELECT m.id, m.kind, m.text, m.from_me, c.jid, m.quoted_message_id, m.starred, m.system_action
         FROM message m LEFT JOIN contact c ON c.id = m.sender_id WHERE m.key_id = ?1",
        [key],
        |r| {
            Ok(MsgRow {
                id: r.get(0)?,
                kind: r.get(1)?,
                text: r.get(2)?,
                from_me: r.get(3)?,
                sender_jid: r.get(4)?,
                quoted_id: r.get(5)?,
                starred: r.get(6)?,
                system_action: r.get(7)?,
            })
        },
    )
    .unwrap()
}

#[derive(Debug)]
#[allow(dead_code)]
struct MsgRow {
    id: i64,
    kind: String,
    text: Option<String>,
    from_me: bool,
    sender_jid: Option<String>,
    quoted_id: Option<i64>,
    starred: bool,
    system_action: Option<i64>,
}

fn search(dst: &Connection, q: &str) -> Vec<String> {
    dst.prepare("SELECT m.key_id FROM message_fts f JOIN message m ON m.id = f.rowid WHERE message_fts MATCH ?1 ORDER BY m.id")
        .unwrap()
        .query_map([q], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

// --- Schema novo ------------------------------------------------------------

#[test]
fn moderno_relatorio() {
    let (_, report) = run(&source("modern.sql"));
    assert_eq!(
        report,
        ImportReport {
            source_schema: Some(SourceSchema::Modern),
            chats: 2,
            messages: 12,
            media: 2,
            reactions: 2,
            vcards: 1,
            skipped_status: 1,
            skipped_orphans: 1,
            unknown_types: BTreeMap::from([(777, 1)]),
        }
    );
}

#[test]
fn moderno_conversas() {
    let (dst, _) = run(&source("modern.sql"));
    let chats: Vec<(String, String, Option<String>, bool, i64, i64)> = dst
        .prepare(
            "SELECT c.jid, ch.kind, ch.title, ch.archived, ch.message_count, ch.last_message_at
             FROM chat ch JOIN contact c ON c.id = ch.contact_id ORDER BY ch.id",
        )
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(
        chats,
        vec![
            ("5511911111111@s.whatsapp.net".into(), "direct".into(), None, false, 9, 14000),
            ("120363000000000001@g.us".into(), "group".into(), Some("Família".into()), true, 3, 9000),
        ]
    );
}

#[test]
fn moderno_lid_vira_telefone() {
    let (dst, _) = run(&source("modern.sql"));
    // Bob escreveu com LID, mapeado via jid_map para o telefone.
    let bob = msg(&dst, "G1");
    assert_eq!(bob.sender_jid.as_deref(), Some("5511922222222@s.whatsapp.net"));
    let phone: Option<String> = dst
        .query_row("SELECT phone FROM contact WHERE jid = '5511922222222@s.whatsapp.net'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(phone.as_deref(), Some("5511922222222"));
    // Carol não tem mapeamento: fica o LID, sem telefone.
    let carol = msg(&dst, "G2");
    assert_eq!(carol.sender_jid.as_deref(), Some("55555@lid"));
    // O LID do Bob não vira um contato separado.
    let lid_contacts: i64 = dst
        .query_row("SELECT count(*) FROM contact WHERE jid = '98765@lid'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(lid_contacts, 0);
}

#[test]
fn moderno_tipos_e_apagadas() {
    let (dst, _) = run(&source("modern.sql"));
    assert_eq!(msg(&dst, "A1").kind, "text");
    assert_eq!(msg(&dst, "A3").kind, "image");
    assert_eq!(msg(&dst, "A3").text.as_deref(), Some("foto da praia"));
    assert_eq!(msg(&dst, "A4").kind, "voice");
    assert_eq!(msg(&dst, "A7").kind, "location");
    assert_eq!(msg(&dst, "A8").kind, "contact");
    assert_eq!(msg(&dst, "A9").kind, "unknown");
    let sys = msg(&dst, "G3");
    assert_eq!((sys.kind.as_str(), sys.system_action), ("system", Some(4)));

    // Apagadas por tipo 15 e por edit_version 7: sem texto.
    for key in ["A5", "A6"] {
        let m = msg(&dst, key);
        assert_eq!(m.kind, "deleted", "{key}");
        assert_eq!(m.text, None, "{key}");
    }
    assert!(search(&dst, "segredo").is_empty());
    assert!(search(&dst, "antiga").is_empty());
}

#[test]
fn moderno_resposta_aponta_para_original() {
    let (dst, _) = run(&source("modern.sql"));
    let reply = msg(&dst, "A2");
    assert_eq!(reply.quoted_id, Some(msg(&dst, "A1").id));
    assert!(reply.starred && reply.from_me);
    let quoted_text: Option<String> = dst
        .query_row("SELECT quoted_text FROM message WHERE key_id = 'A2'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(quoted_text.as_deref(), Some("Oi, tudo bem? Vamos tomar um café"));
}

#[test]
fn moderno_midia_miniatura_local_vcard() {
    let (dst, _) = run(&source("modern.sql"));
    let img = msg(&dst, "A3").id;
    let (path, w, h, size): (String, i64, i64, i64) = dst
        .query_row("SELECT file_path, width, height, size FROM media WHERE message_id = ?1", [img], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })
        .unwrap();
    assert_eq!((path.as_str(), w, h, size), ("Media/WhatsApp Images/IMG-0001.jpg", 1600, 1200, 12345));
    let thumb: Vec<u8> = dst
        .query_row("SELECT data FROM thumbnail WHERE message_id = ?1", [img], |r| r.get(0))
        .unwrap();
    assert!(thumb.starts_with(&[0xFF, 0xD8, 0xFF]));

    // Mídia sem dimensão (0) vira NULL, para o viewer não reservar 0 px.
    let voice = msg(&dst, "A4").id;
    let (vw, transcription): (Option<i64>, Option<String>) = dst
        .query_row("SELECT width, transcription FROM media WHERE message_id = ?1", [voice], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!((vw, transcription.as_deref()), (None, Some("chego em dez minutos")));

    let (lat, name): (f64, String) = dst
        .query_row("SELECT latitude, name FROM location WHERE message_id = ?1", [msg(&dst, "A7").id], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!((lat, name.as_str()), (-23.5614, "Av. Paulista"));

    let card: String = dst
        .query_row("SELECT data FROM vcard WHERE message_id = ?1", [msg(&dst, "A8").id], |r| r.get(0))
        .unwrap();
    assert!(card.contains("FN:Zé"));
}

#[test]
fn moderno_reacoes() {
    let (dst, _) = run(&source("modern.sql"));
    let rows: Vec<(String, String, bool, Option<String>)> = dst
        .prepare(
            "SELECT m.key_id, r.emoji, r.from_me, c.jid FROM reaction r
             JOIN message m ON m.id = r.message_id LEFT JOIN contact c ON c.id = r.sender_id ORDER BY r.id",
        )
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    // A reação removida (emoji vazio) da Carol não entra.
    assert_eq!(
        rows,
        vec![
            ("A1".into(), "❤️".into(), true, None),
            ("G1".into(), "👍".into(), false, Some("5511922222222@s.whatsapp.net".into())),
        ]
    );
}

#[test]
fn busca_ignora_acentos() {
    let (dst, _) = run(&source("modern.sql"));
    assert_eq!(search(&dst, "cafe"), vec!["A1"]);
    assert_eq!(search(&dst, "CAFÉ"), vec!["A1"]);
    assert_eq!(search(&dst, "ola"), vec!["G1"]);
    assert_eq!(search(&dst, "praia"), vec!["A3"]); // legenda de foto
    assert!(search(&dst, "status").is_empty()); // stories não são importados
}

#[test]
fn moderno_sem_tabelas_opcionais() {
    // Versões antigas do schema novo não têm jid_map, reações, futuro...
    let src = Connection::open_in_memory().unwrap();
    src.execute_batch(
        "CREATE TABLE jid (_id INTEGER PRIMARY KEY, user TEXT, server TEXT);
         CREATE TABLE chat (_id INTEGER PRIMARY KEY, jid_row_id INTEGER);
         CREATE TABLE message (_id INTEGER PRIMARY KEY, chat_row_id INTEGER, from_me INTEGER, key_id TEXT, timestamp INTEGER);
         INSERT INTO jid VALUES (1, '5511900000000', 's.whatsapp.net');
         INSERT INTO chat VALUES (1, 1);
         INSERT INTO message VALUES (1, 1, 0, 'K1', 1000);",
    )
    .unwrap();
    let (dst, report) = run(&src);
    assert_eq!((report.chats, report.messages), (1, 1));
    assert_eq!(msg(&dst, "K1").kind, "text");
}

#[test]
fn metadados_e_destino_ja_usado() {
    let src = source("modern.sql");
    let mut dst = Connection::open_in_memory().unwrap();
    import(
        &src,
        &mut dst,
        &ImportOptions {
            source_app: SourceApp::Business,
        },
    )
    .unwrap();
    let meta = |k: &str| -> Option<String> {
        dst.query_row("SELECT value FROM meta WHERE key = ?1", [k], |r| r.get(0))
            .optional()
            .unwrap()
    };
    assert_eq!(meta("source_app").as_deref(), Some("business"));
    assert_eq!(meta("source_schema").as_deref(), Some("modern"));
    assert_eq!(meta("schema_version").as_deref(), Some("1"));
    assert!(matches!(import(&src, &mut dst, &opts()), Err(Error::DestinationNotEmpty)));
}

// --- Schema legado ----------------------------------------------------------

#[test]
fn legado_relatorio() {
    let (_, report) = run(&source("legacy.sql"));
    assert_eq!(
        report,
        ImportReport {
            source_schema: Some(SourceSchema::Legacy),
            chats: 2,
            messages: 8,
            media: 1,
            reactions: 0,
            vcards: 1,
            skipped_status: 1,
            skipped_orphans: 0,
            unknown_types: BTreeMap::new(),
        }
    );
}

#[test]
fn legado_conteudo() {
    let (dst, _) = run(&source("legacy.sql"));
    let reply = msg(&dst, "L2");
    assert_eq!(reply.quoted_id, Some(msg(&dst, "L1").id));
    assert_eq!(msg(&dst, "L3").sender_jid.as_deref(), Some("5511933333333@s.whatsapp.net"));

    let img = msg(&dst, "L4");
    assert_eq!((img.kind.as_str(), img.text.as_deref()), ("image", Some("pôr do sol")));
    let has_thumb: bool = dst
        .query_row("SELECT EXISTS(SELECT 1 FROM thumbnail WHERE message_id = ?1)", [img.id], |r| r.get(0))
        .unwrap();
    assert!(has_thumb);

    let contact = msg(&dst, "L5");
    assert_eq!((contact.kind.as_str(), contact.text.as_deref()), ("contact", Some("Zé")));
    assert_eq!(msg(&dst, "L6").kind, "system");
    let deleted = msg(&dst, "L7");
    assert_eq!((deleted.kind.as_str(), deleted.text), ("deleted", None));
    assert_eq!(msg(&dst, "L9").kind, "location");

    assert_eq!(search(&dst, "cafe"), vec!["L1"]);
    assert_eq!(search(&dst, "por do sol"), vec!["L4"]);
}

// --- Entrada ----------------------------------------------------------------

#[test]
fn abre_msgstore_da_memoria() {
    let mut src = source("modern.sql");
    let bytes = src.serialize(MAIN_DB).unwrap().to_vec();
    let conn = open_msgstore(&bytes).unwrap();
    // O SQLCipher devolve o valor do pragma como texto.
    let on: String = conn
        .query_row("PRAGMA cipher_memory_security", [], |r| r.get(0))
        .unwrap();
    assert_eq!(on, "1", "SQLCipher deve zerar a memória liberada");
    // Somente leitura.
    assert!(conn.execute("DELETE FROM message", []).is_err());
    let (_, report) = run(&conn);
    assert_eq!(report.messages, 12);
}

#[test]
fn rejeita_o_que_nao_e_msgstore() {
    assert!(open_msgstore(b"isto nao e um banco sqlite, so texto qualquer").is_err());
    let src = Connection::open_in_memory().unwrap();
    src.execute_batch("CREATE TABLE notas (texto TEXT);").unwrap();
    let mut dst = Connection::open_in_memory().unwrap();
    assert!(matches!(import(&src, &mut dst, &opts()), Err(Error::NotMsgstore)));
}

/// Ponta a ponta: .crypt15 → wa-crypt → memória → importer.
/// O fixture foi gerado a partir de `modern.sql` com o wa-crypt-tools (waencrypt).
#[test]
fn ponta_a_ponta_crypt15() {
    let dir = env!("CARGO_MANIFEST_DIR");
    let file = std::fs::read(format!("{dir}/tests/fixtures/msgstore-modern.db.crypt15")).unwrap();
    let hex = std::fs::read_to_string(format!("{dir}/../wa-crypt/tests/fixtures/key15.hex")).unwrap();
    let key = wa_crypt::BackupKey::from_hex(&hex).unwrap();
    let decrypted = wa_crypt::decrypt(&file, &key).unwrap();
    assert_eq!(decrypted.payload, wa_crypt::Payload::Sqlite);
    let src = open_msgstore(&decrypted.data).unwrap();
    drop(decrypted); // o texto puro fica só dentro do SQLite (zerado ao fechar)
    let (dst, report) = run(&src);
    assert_eq!(report.messages, 12);
    assert_eq!(search(&dst, "cafe"), vec!["A1"]);
}

/// `cargo test -p wa-import --release -- --ignored desempenho --nocapture`
#[test]
#[ignore]
fn desempenho_300k_mensagens() {
    let src = source("modern.sql");
    src.execute_batch(
        "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 300000)
         INSERT INTO message (chat_row_id, from_me, key_id, sender_jid_row_id, status, timestamp, message_type, text_data)
         SELECT 1 + (i % 2), i % 2, 'P' || i, CASE i % 2 WHEN 1 THEN 4 ELSE 0 END, 13, 100000 + i, 0,
                'mensagem número ' || i || ' com um texto de tamanho médio, parecido com conversas reais'
         FROM n;
         INSERT INTO message_quoted (message_row_id, chat_row_id, key_id, text_data)
         SELECT _id, chat_row_id, 'P' || (CAST(substr(key_id, 2) AS INTEGER) - 2), 'citação'
         FROM message WHERE key_id LIKE 'P%' AND _id % 10 = 0;",
    )
    .unwrap();
    let start = std::time::Instant::now();
    let (dst, report) = run(&src);
    let elapsed = start.elapsed();
    let linked: i64 = dst
        .query_row("SELECT count(*) FROM message WHERE quoted_message_id IS NOT NULL", [], |r| r.get(0))
        .unwrap();
    let t = std::time::Instant::now();
    let hits = search(&dst, "numero 123456").len();
    println!(
        "{} mensagens em {:.2?} ({:.0}/s), {linked} citações ligadas, busca em {:.2?} ({hits} resultado)",
        report.messages,
        elapsed,
        report.messages as f64 / elapsed.as_secs_f64(),
        t.elapsed()
    );
    assert_eq!(report.messages, 300_012);
}
