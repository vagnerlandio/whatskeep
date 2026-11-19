//! Vetores gerados por uma implementação independente (wa-crypt-tools 0.1.0):
//!
//! ```text
//! waencrypt --type 15 <hex>  plain.db msgstore.db.crypt15
//! waencrypt --type 14 key    plain.db msgstore.db.crypt14
//! wacreatekey --hex <hex>            → encrypted_backup.key
//! wacreatekey -c14 --hex <hex>       → key
//! ```
//!
//! Todas as chaves são aleatórias e `plain.db` é sintético.

use wa_crypt::{decrypt, BackupKey, CryptVersion, Error, KeyKind, Payload};

fn fx(name: &str) -> Vec<u8> {
    std::fs::read(format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

fn key15_hex() -> String {
    String::from_utf8(fx("key15.hex")).unwrap()
}

#[test]
fn crypt15_com_chave_de_64_digitos() {
    let key = BackupKey::from_hex(&key15_hex()).unwrap();
    let out = decrypt(&fx("msgstore.db.crypt15"), &key).unwrap();
    assert_eq!(out.version, CryptVersion::Crypt15);
    assert_eq!(out.payload, Payload::Sqlite);
    assert!(!out.multifile);
    assert_eq!(&out.data[..], &fx("plain.db")[..]);
}

#[test]
fn crypt15_com_encrypted_backup_key() {
    let key = BackupKey::from_key_file(&fx("encrypted_backup.key")).unwrap();
    assert_eq!(key.kind(), KeyKind::Crypt15);
    let out = decrypt(&fx("msgstore.db.crypt15"), &key).unwrap();
    assert_eq!(&out.data[..], &fx("plain.db")[..]);
}

#[test]
fn crypt15_multifile_sem_md5() {
    // O multi-file é [header][ct][tag], sem md5 final. O wa-crypt-tools 0.1.0
    // ignora `--multi-file`, então o caso é derivado removendo o md5.
    let key = BackupKey::from_hex(&key15_hex()).unwrap();
    let f = fx("msgstore.db.crypt15");
    let out = decrypt(&f[..f.len() - 16], &key).unwrap();
    assert!(out.multifile);
    assert_eq!(&out.data[..], &fx("plain.db")[..]);
}

#[test]
fn crypt14_com_arquivo_key() {
    let key = BackupKey::from_key_file(&fx("key")).unwrap();
    assert_eq!(key.kind(), KeyKind::Crypt14);
    let out = decrypt(&fx("msgstore.db.crypt14"), &key).unwrap();
    assert_eq!(out.version, CryptVersion::Crypt14);
    assert_eq!(&out.data[..], &fx("plain.db")[..]);
}

#[test]
fn chave_errada_falha_na_autenticacao() {
    let key = BackupKey::from_hex(&"00".repeat(32)).unwrap();
    assert!(matches!(
        decrypt(&fx("msgstore.db.crypt15"), &key),
        Err(Error::Authentication)
    ));
}

#[test]
fn qualquer_byte_adulterado_falha() {
    let key = BackupKey::from_hex(&key15_hex()).unwrap();
    let original = fx("msgstore.db.crypt15");
    // Ignora o header: lá, mudar bytes do IV/protobuf pode dar outros erros,
    // o que também é aceitável. Aqui interessa o corpo cifrado, a tag e o md5.
    let start = wa_crypt::parse_prefix(&original).unwrap().header_len;
    for i in (start..original.len()).step_by(7) {
        let mut f = original.clone();
        f[i] ^= 0x01;
        assert!(
            matches!(decrypt(&f, &key), Err(Error::Authentication)),
            "byte {i} adulterado não foi detectado"
        );
    }
}

#[test]
fn iv_adulterado_falha() {
    let key = BackupKey::from_hex(&key15_hex()).unwrap();
    let mut f = fx("msgstore.db.crypt15");
    // varint 85 01 | 08 01 | 1a 12 | 0a 10 | IV...
    f[8] ^= 0xFF;
    assert!(decrypt(&f, &key).is_err());
}

#[test]
fn tipo_de_chave_trocado() {
    let k14 = BackupKey::from_key_file(&fx("key")).unwrap();
    assert!(matches!(
        decrypt(&fx("msgstore.db.crypt15"), &k14),
        Err(Error::KeyMismatch { key: KeyKind::Crypt14, file: CryptVersion::Crypt15 })
    ));
}

#[test]
fn arquivo_truncado() {
    let key = BackupKey::from_hex(&key15_hex()).unwrap();
    let f = fx("msgstore.db.crypt15");
    let header = wa_crypt::parse_prefix(&f).unwrap().header_len;
    assert!(matches!(decrypt(&f[..header + 10], &key), Err(Error::Truncated)));
    assert!(decrypt(&f[..f.len() - 1], &key).is_err());
}

#[test]
fn arquivo_que_nao_e_backup() {
    let key = BackupKey::from_hex(&key15_hex()).unwrap();
    assert!(decrypt(&fx("plain.db"), &key).is_err());
}
