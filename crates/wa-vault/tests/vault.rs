use std::fs;
use std::path::Path;

use rusqlite::Connection;
use wa_import::SourceApp;
use wa_vault::{Error, KdfParams, Vault};

const FAST: KdfParams = KdfParams::MINIMUM;
const PW: &str = "senha forte 123";

fn msgstore() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(include_str!("../../wa-import/tests/fixtures/modern.sql")).unwrap();
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
fn cria_destrava_e_recusa_senha_errada() {
    let dir = tempfile::tempdir().unwrap();
    assert!(!Vault::exists(dir.path()));
    drop(Vault::create(dir.path(), PW, FAST).unwrap());
    assert!(Vault::exists(dir.path()));

    Vault::unlock(dir.path(), PW).unwrap();
    assert!(matches!(Vault::unlock(dir.path(), "senha errada!"), Err(Error::WrongPassword)));
    assert!(matches!(Vault::create(dir.path(), PW, FAST), Err(Error::AlreadyExists)));
}

#[test]
fn senha_curta_e_recusada() {
    let dir = tempfile::tempdir().unwrap();
    assert!(matches!(Vault::create(dir.path(), "1234567", FAST), Err(Error::WeakPassword)));
    // Conta caracteres, não bytes.
    assert!(matches!(Vault::create(dir.path(), "çãéíóúâ", FAST), Err(Error::WeakPassword)));
    Vault::create(dir.path(), "çãéíóúâê", FAST).unwrap();
}

#[test]
fn sem_cofre_retorna_not_found() {
    let dir = tempfile::tempdir().unwrap();
    assert!(matches!(Vault::unlock(dir.path(), PW), Err(Error::NotFound)));
}

#[test]
fn cabecalho_adulterado_nao_abre() {
    let dir = tempfile::tempdir().unwrap();
    drop(Vault::create(dir.path(), PW, KdfParams { m_kib: 16 * 1024, t: 2, p: 1 }).unwrap());
    let path = dir.path().join("vault.json");
    let original = fs::read_to_string(&path).unwrap();

    // Baixar o custo do KDF muda o AAD: a tag GCM não confere.
    fs::write(&path, original.replace("\"t\": 2", "\"t\": 1")).unwrap();
    assert!(matches!(Vault::unlock(dir.path(), PW), Err(Error::WrongPassword)));

    // Parâmetros absurdos são recusados antes de rodar o Argon2id.
    fs::write(&path, original.replace("\"m_kib\": 16384", "\"m_kib\": 99999999")).unwrap();
    assert!(matches!(Vault::unlock(dir.path(), PW), Err(Error::Corrupt(_))));

    fs::write(&path, "não é json").unwrap();
    assert!(matches!(Vault::unlock(dir.path(), PW), Err(Error::Corrupt(_))));

    fs::write(&path, &original).unwrap();
    Vault::unlock(dir.path(), PW).unwrap();
}

#[test]
fn importa_e_le_depois_de_travar_e_destravar() {
    let dir = tempfile::tempdir().unwrap();
    let vault = Vault::create(dir.path(), PW, FAST).unwrap();
    assert!(vault.stores().unwrap().is_empty());

    let store = vault.store(SourceApp::WhatsApp).unwrap();
    assert!(store.open().unwrap().is_none());
    let report = store.import(&msgstore()).unwrap();
    assert!(report.messages > 0);
    drop(store);
    drop(vault); // trava

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
fn nada_em_claro_no_disco() {
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
            assert!(!contains(bytes, needle), "{path} contém {:?}", String::from_utf8_lossy(needle));
        }
    }
}

#[cfg(unix)]
#[test]
fn permissoes_restritas() {
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
fn bancos_tem_chaves_diferentes() {
    let dir = tempfile::tempdir().unwrap();
    let vault = Vault::create(dir.path(), PW, FAST).unwrap();
    vault.store(SourceApp::WhatsApp).unwrap().import(&msgstore()).unwrap();

    // O banco do Business não abre com a chave do WhatsApp.
    let stores = dir.path().join("stores");
    fs::copy(stores.join("whatsapp.db"), stores.join("business.db")).unwrap();
    assert!(matches!(
        vault.store(SourceApp::Business).unwrap().open(),
        Err(Error::StoreUnreadable)
    ));

    // Nem em outro cofre com a mesma senha (DEK e id diferentes).
    let other = tempfile::tempdir().unwrap();
    let vault2 = Vault::create(other.path(), PW, FAST).unwrap();
    fs::copy(stores.join("whatsapp.db"), other.path().join("stores/whatsapp.db")).unwrap();
    assert!(matches!(
        vault2.store(SourceApp::WhatsApp).unwrap().open(),
        Err(Error::StoreUnreadable)
    ));
}

#[test]
fn reimportar_substitui() {
    let dir = tempfile::tempdir().unwrap();
    let vault = Vault::create(dir.path(), PW, FAST).unwrap();
    let store = vault.store(SourceApp::WhatsApp).unwrap();
    let first = store.import(&msgstore()).unwrap();
    let second = store.import(&msgstore()).unwrap();
    assert_eq!(first, second);
    assert_eq!(vault.stores().unwrap()[0].messages, second.messages);
}

#[test]
fn importacao_com_falha_preserva_o_banco_anterior() {
    let dir = tempfile::tempdir().unwrap();
    let vault = Vault::create(dir.path(), PW, FAST).unwrap();
    let store = vault.store(SourceApp::WhatsApp).unwrap();
    let report = store.import(&msgstore()).unwrap();

    let not_msgstore = Connection::open_in_memory().unwrap();
    not_msgstore.execute_batch("CREATE TABLE foo (x)").unwrap();
    assert!(matches!(store.import(&not_msgstore), Err(Error::Import(wa_import::Error::NotMsgstore))));

    assert_eq!(vault.stores().unwrap()[0].messages, report.messages);
    let leftovers: Vec<_> = fs::read_dir(dir.path().join("stores"))
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(leftovers, ["whatsapp.db"]);
}

#[test]
fn temporarios_antigos_sao_limpos_ao_destravar() {
    let dir = tempfile::tempdir().unwrap();
    drop(Vault::create(dir.path(), PW, FAST).unwrap());
    let stale = dir.path().join("stores/whatsapp.db.tmp-0011aabbccdd");
    fs::write(&stale, b"lixo cifrado").unwrap();
    Vault::unlock(dir.path(), PW).unwrap();
    assert!(!stale.exists());
}

#[test]
fn troca_de_senha() {
    let dir = tempfile::tempdir().unwrap();
    let mut vault = Vault::create(dir.path(), PW, FAST).unwrap();
    vault.store(SourceApp::WhatsApp).unwrap().import(&msgstore()).unwrap();

    assert!(matches!(
        vault.change_password("chute qualquer", "nova senha 456", FAST),
        Err(Error::WrongPassword)
    ));
    assert!(matches!(vault.change_password(PW, "curta", FAST), Err(Error::WeakPassword)));
    vault.change_password(PW, "nova senha 456", FAST).unwrap();
    drop(vault);

    assert!(matches!(Vault::unlock(dir.path(), PW), Err(Error::WrongPassword)));
    let vault = Vault::unlock(dir.path(), "nova senha 456").unwrap();
    // Mesma DEK: os bancos continuam abrindo sem recifrar.
    assert_eq!(vault.stores().unwrap().len(), 1);
}

#[test]
fn debug_nao_vaza_chave() {
    let dir = tempfile::tempdir().unwrap();
    let vault = Vault::create(dir.path(), PW, FAST).unwrap();
    let dbg = format!("{vault:?} {:?}", vault.store(SourceApp::WhatsApp).unwrap());
    assert!(dbg.contains("<redacted>"));
    assert!(!dbg.contains("key: ["));
}

/// Mede o Argon2id com os parâmetros padrão:
/// `cargo test -p wa-vault --release -- --ignored --nocapture`
#[test]
#[ignore]
fn tempo_do_kdf_padrao() {
    let dir = tempfile::tempdir().unwrap();
    drop(Vault::create(dir.path(), PW, KdfParams::DEFAULT).unwrap());
    let t = std::time::Instant::now();
    Vault::unlock(dir.path(), PW).unwrap();
    println!("unlock com KdfParams::DEFAULT: {:?}", t.elapsed());
}

#[cfg(target_os = "macos")]
#[test]
fn fora_do_time_machine() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("vault");
    drop(Vault::create(&root, PW, FAST).unwrap());
    let out = std::process::Command::new("tmutil").arg("isexcluded").arg(&root).output().unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("[Excluded]"), "{text}");
}
