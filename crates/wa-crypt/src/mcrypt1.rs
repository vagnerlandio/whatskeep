//! Mídias de backups E2E no Google Drive (`<hash>.mcrypt1`).
//!
//! Formato reconstruído a partir do wabdd e do wa-crypt-tools (não há
//! documentação oficial):
//!
//! * **Nome:** `<hex>.mcrypt1`, onde o hex é um HMAC do nome real e do MD5.
//! * **Metadados** (campo `metadata` da API, base64): `[16][iv 16][32][mac 32][AES-256-CBC]`,
//!   com MAC = HMAC-SHA256(chave "metadata authentication", iv ‖ cifrado) e
//!   chave de cifra "metadata encryption". O conteúdo é um JSON com `name`.
//! * **Conteúdo:** AES-256-GCM com chave e IV (16 bytes) de
//!   HKDF(root, info = bytes do hex do nome, L = 48).
//!
//! As referências decifram o conteúdo **sem verificar a tag GCM**, e não está
//! documentado se a tag vem no fim do arquivo. Por isso [`decrypt_media`]
//! tenta primeiro com a tag nos últimos 16 bytes e, se não conferir, decifra
//! sem autenticação e devolve `verified = false`. Nesse caso, quem chama deve
//! conferir o resultado por outro meio (o SHA-256 que o msgstore guarda de
//! cada mídia) antes de confiar nele.

use aes::Aes256;
use aes_gcm::aead::consts::U16;
use aes_gcm::aead::generic_array::GenericArray;
use aes_gcm::{AeadInPlace, AesGcm, KeyInit};
use cbc::cipher::{BlockDecryptMut, KeyIvInit, block_padding::Pkcs7};
use ctr::cipher::StreamCipher;
use ghash::universal_hash::UniversalHash;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use zeroize::Zeroizing;

use crate::{BackupKey, Error};

type HmacSha256 = Hmac<Sha256>;
type WaGcm = AesGcm<Aes256, U16>;

/// HKDF-SHA256 com salt de 32 zeros (o `encryptionloop` do wa-crypt-tools).
fn expand(root: &[u8; 32], info: &[u8], out: &mut [u8]) {
    let mut extract = <HmacSha256 as Mac>::new_from_slice(&[0u8; 32]).expect("HMAC aceita qualquer tamanho");
    extract.update(root);
    let prk = Zeroizing::new(<[u8; 32]>::from(extract.finalize().into_bytes()));
    let mut prev: Zeroizing<Vec<u8>> = Zeroizing::new(Vec::new());
    for (i, chunk) in out.chunks_mut(32).enumerate() {
        let mut h = <HmacSha256 as Mac>::new_from_slice(&*prk).expect("HMAC aceita qualquer tamanho");
        h.update(&prev);
        h.update(info);
        h.update(&[(i + 1) as u8]);
        let block = Zeroizing::new(<[u8; 32]>::from(h.finalize().into_bytes()));
        chunk.copy_from_slice(&block[..chunk.len()]);
        prev = Zeroizing::new(block.to_vec());
    }
}

fn root(key: &BackupKey) -> Result<&[u8; 32], Error> {
    key.root().ok_or(Error::InvalidKey("mídias do Google Drive exigem a chave de 64 dígitos (crypt15)"))
}

/// Decifra os metadados (já decodificados do base64) e devolve o JSON.
pub fn decrypt_metadata(key: &BackupKey, raw: &[u8]) -> Result<Zeroizing<Vec<u8>>, Error> {
    let root = root(key)?;
    if raw.len() < 50 + 16 || raw[0] != 16 || raw[17] != 32 {
        return Err(Error::Header("metadados do .mcrypt1 em formato desconhecido"));
    }
    let (iv, mac, body) = (&raw[1..17], &raw[18..50], &raw[50..]);

    let mut auth_key = Zeroizing::new([0u8; 32]);
    expand(root, b"metadata authentication", &mut *auth_key);
    let mut h = <HmacSha256 as Mac>::new_from_slice(&*auth_key).expect("HMAC aceita qualquer tamanho");
    h.update(iv);
    h.update(body);
    h.verify_slice(mac).map_err(|_| Error::Authentication)?;

    let mut enc_key = Zeroizing::new([0u8; 32]);
    expand(root, b"metadata encryption", &mut *enc_key);
    let mut buf = Zeroizing::new(body.to_vec());
    let len = cbc::Decryptor::<Aes256>::new(enc_key.as_ref().into(), iv.into())
        .decrypt_padded_mut::<Pkcs7>(&mut buf)
        .map_err(|_| Error::Authentication)?
        .len();
    buf.truncate(len);
    Ok(buf)
}

pub struct Media {
    pub data: Zeroizing<Vec<u8>>,
    /// `true` se a tag GCM conferiu. `false`: decifrado sem autenticação
    /// (confira o hash antes de usar).
    pub verified: bool,
}

/// Decifra o conteúdo de um `.mcrypt1`. `name_hex` é o nome do arquivo sem a extensão.
pub fn decrypt_media(key: &BackupKey, name_hex: &str, mut data: Vec<u8>) -> Result<Media, Error> {
    let root = root(key)?;
    let info = decode_hex(name_hex).ok_or(Error::Header("nome de .mcrypt1 não é hexadecimal"))?;
    let mut km = Zeroizing::new([0u8; 48]);
    expand(root, &info, &mut *km);
    let (aes_key, iv) = km.split_at(32);

    // 1) Tag GCM nos últimos 16 bytes.
    if data.len() >= 16 {
        let split = data.len() - 16;
        let tag = GenericArray::clone_from_slice(&data[split..]);
        let mut body = Zeroizing::new(data[..split].to_vec());
        if WaGcm::new(GenericArray::from_slice(aes_key))
            .decrypt_in_place_detached(GenericArray::from_slice(iv), b"", &mut body, &tag)
            .is_ok()
        {
            data.iter_mut().for_each(|b| *b = 0);
            return Ok(Media { data: body, verified: true });
        }
    }

    // 2) Sem tag (ou tag em outro lugar): só o keystream do GCM (AES-CTR a
    //    partir de inc32(J0)), como fazem as referências.
    gcm_keystream(aes_key, iv, &mut data);
    Ok(Media { data: Zeroizing::new(data), verified: false })
}

/// Aplica o keystream do AES-GCM (nonce de 16 bytes) sem verificar nada.
fn gcm_keystream(key: &[u8], iv: &[u8], data: &mut [u8]) {
    use aes::cipher::BlockEncrypt;
    // H = AES_K(0^128)
    let cipher = Aes256::new(GenericArray::from_slice(key));
    let mut h = GenericArray::from([0u8; 16]);
    cipher.encrypt_block(&mut h);
    // J0 = GHASH_H(IV ‖ 0^64 ‖ [len(IV) em bits]_64), para IV ≠ 96 bits.
    let mut gh = ghash::GHash::new(&h);
    let mut len_block = [0u8; 16];
    len_block[8..].copy_from_slice(&((iv.len() as u64) * 8).to_be_bytes());
    gh.update_padded(iv);
    gh.update(&[GenericArray::from(len_block)]);
    let mut counter = gh.finalize();
    // inc32
    let c = u32::from_be_bytes(counter[12..16].try_into().unwrap()).wrapping_add(1);
    counter[12..16].copy_from_slice(&c.to_be_bytes());
    let mut ctr = ctr::Ctr32BE::<Aes256>::new(GenericArray::from_slice(key), &counter);
    ctr.apply_keystream(data);
}

fn decode_hex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok()).collect()
}

#[cfg(test)]
mod tests {
    use cbc::cipher::BlockEncryptMut;

    use super::*;

    fn key() -> BackupKey {
        BackupKey::from_hex("6730a595a1484d0c39c101dc0ac82ec5e401bb6f0e1b8ee2dc104a6b3687f017").unwrap()
    }
    const ROOT: [u8; 32] = [
        0x67, 0x30, 0xa5, 0x95, 0xa1, 0x48, 0x4d, 0x0c, 0x39, 0xc1, 0x01, 0xdc, 0x0a, 0xc8, 0x2e, 0xc5, 0xe4, 0x01,
        0xbb, 0x6f, 0x0e, 0x1b, 0x8e, 0xe2, 0xdc, 0x10, 0x4a, 0x6b, 0x36, 0x87, 0xf0, 0x17,
    ];

    /// Cifra como o WhatsApp (pelo que as referências decifram).
    fn encrypt_media(name_hex: &str, plain: &[u8]) -> (Vec<u8>, [u8; 16]) {
        let mut km = [0u8; 48];
        expand(&ROOT, &decode_hex(name_hex).unwrap(), &mut km);
        let mut buf = plain.to_vec();
        let tag = WaGcm::new(GenericArray::from_slice(&km[..32]))
            .encrypt_in_place_detached(GenericArray::from_slice(&km[32..]), b"", &mut buf)
            .unwrap();
        (buf, tag.into())
    }

    #[test]
    fn expand_igual_ao_derive_do_crypt15() {
        // O primeiro bloco com info "backup encryption" é a chave do crypt15.
        let mut out = [0u8; 32];
        expand(&ROOT, b"backup encryption", &mut out);
        assert_eq!(&out, key().aes_key());
    }

    #[test]
    fn midia_com_tag_no_fim() {
        let name = "00112233445566778899aabbccddeeff";
        let plain = b"\xFF\xD8\xFF\xE0 uma foto qualquer".repeat(100);
        let (mut ct, tag) = encrypt_media(name, &plain);
        ct.extend_from_slice(&tag);
        let m = decrypt_media(&key(), name, ct).unwrap();
        assert!(m.verified);
        assert_eq!(&*m.data, &plain);
    }

    #[test]
    fn midia_sem_tag_decifra_sem_verificacao() {
        let name = "a1b2c3";
        let plain = b"conteudo sem tag".repeat(77);
        let (ct, _) = encrypt_media(name, &plain);
        let m = decrypt_media(&key(), name, ct).unwrap();
        assert!(!m.verified);
        assert_eq!(&*m.data, &plain, "o keystream manual tem que bater com o do AES-GCM");
    }

    #[test]
    fn metadados() {
        let json = br#"{"name":"Media/WhatsApp Images/IMG-1.jpg","updateTime":"2026-01-01T00:00:00Z"}"#;
        let iv = [7u8; 16];
        let mut enc_key = [0u8; 32];
        expand(&ROOT, b"metadata encryption", &mut enc_key);
        let body = cbc::Encryptor::<Aes256>::new((&enc_key).into(), (&iv).into()).encrypt_padded_vec_mut::<Pkcs7>(json);
        let mut auth_key = [0u8; 32];
        expand(&ROOT, b"metadata authentication", &mut auth_key);
        let mut h = <HmacSha256 as Mac>::new_from_slice(&auth_key).unwrap();
        h.update(&iv);
        h.update(&body);
        let mac = h.finalize().into_bytes();
        let mut raw = vec![16u8];
        raw.extend_from_slice(&iv);
        raw.push(32);
        raw.extend_from_slice(&mac);
        raw.extend_from_slice(&body);

        assert_eq!(&*decrypt_metadata(&key(), &raw).unwrap(), json);
        let mut bad = raw.clone();
        *bad.last_mut().unwrap() ^= 1;
        assert!(matches!(decrypt_metadata(&key(), &bad), Err(Error::Authentication)));
        assert!(matches!(decrypt_metadata(&key(), &raw[..40]), Err(Error::Header(_))));
    }

    #[test]
    fn chave_crypt14_nao_serve() {
        let data = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/key")).unwrap();
        let k = BackupKey::from_key_file(&data).unwrap();
        assert!(matches!(decrypt_media(&k, "aa", vec![0; 32]), Err(Error::InvalidKey(_))));
    }
}
