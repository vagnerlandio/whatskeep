//! Media of E2E backups on Google Drive (`<hash>.mcrypt1`).
//!
//! Format reconstructed from wabdd and wa-crypt-tools (there is no
//! official documentation):
//!
//! * **Name:** `<hex>.mcrypt1`, where the hex is an HMAC of the real name and the MD5.
//! * **Metadata** (the API's `metadata` field, base64): `[16][iv 16][32][mac 32][AES-256-CBC]`,
//!   with MAC = HMAC-SHA256(key "metadata authentication", iv ‖ ciphertext) and
//!   cipher key "metadata encryption". The content is a JSON with `name`.
//! * **Content:** AES-256-GCM with key and IV (16 bytes) from
//!   HKDF(root, info = bytes of the name's hex, L = 48).
//!
//! The references decrypt the content **without verifying the GCM tag**, and it
//! is not documented whether the tag sits at the end of the file. So [`decrypt_media`]
//! first tries with the tag in the last 16 bytes and, if it does not match, decrypts
//! without authentication and returns `verified = false`. In that case the caller
//! must check the result some other way (the SHA-256 the msgstore keeps for
//! each media file) before trusting it.

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

/// HKDF-SHA256 with a salt of 32 zeros (wa-crypt-tools' `encryptionloop`).
fn expand(root: &[u8; 32], info: &[u8], out: &mut [u8]) {
    let mut extract = <HmacSha256 as Mac>::new_from_slice(&[0u8; 32]).expect("HMAC accepts any key length");
    extract.update(root);
    let prk = Zeroizing::new(<[u8; 32]>::from(extract.finalize().into_bytes()));
    let mut prev: Zeroizing<Vec<u8>> = Zeroizing::new(Vec::new());
    for (i, chunk) in out.chunks_mut(32).enumerate() {
        let mut h = <HmacSha256 as Mac>::new_from_slice(&*prk).expect("HMAC accepts any key length");
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

/// Decrypts the metadata (already base64-decoded) and returns the JSON.
pub fn decrypt_metadata(key: &BackupKey, raw: &[u8]) -> Result<Zeroizing<Vec<u8>>, Error> {
    let root = root(key)?;
    if raw.len() < 50 + 16 || raw[0] != 16 || raw[17] != 32 {
        return Err(Error::Header("metadados do .mcrypt1 em formato desconhecido"));
    }
    let (iv, mac, body) = (&raw[1..17], &raw[18..50], &raw[50..]);

    let mut auth_key = Zeroizing::new([0u8; 32]);
    expand(root, b"metadata authentication", &mut *auth_key);
    let mut h = <HmacSha256 as Mac>::new_from_slice(&*auth_key).expect("HMAC accepts any key length");
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
    /// `true` if the GCM tag matched. `false`: decrypted without authentication
    /// (check the hash before using it).
    pub verified: bool,
}

/// Decrypts the content of a `.mcrypt1`. `name_hex` is the file name without the extension.
pub fn decrypt_media(key: &BackupKey, name_hex: &str, mut data: Vec<u8>) -> Result<Media, Error> {
    let root = root(key)?;
    let info = decode_hex(name_hex).ok_or(Error::Header("nome de .mcrypt1 não é hexadecimal"))?;
    let mut km = Zeroizing::new([0u8; 48]);
    expand(root, &info, &mut *km);
    let (aes_key, iv) = km.split_at(32);

    // 1) GCM tag in the last 16 bytes.
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

    // 2) No tag (or tag somewhere else): just the GCM keystream (AES-CTR
    //    starting at inc32(J0)), as the references do.
    gcm_keystream(aes_key, iv, &mut data);
    Ok(Media { data: Zeroizing::new(data), verified: false })
}

/// Applies the AES-GCM keystream (16-byte nonce) without verifying anything.
fn gcm_keystream(key: &[u8], iv: &[u8], data: &mut [u8]) {
    use aes::cipher::BlockEncrypt;
    // H = AES_K(0^128)
    let cipher = Aes256::new(GenericArray::from_slice(key));
    let mut h = GenericArray::from([0u8; 16]);
    cipher.encrypt_block(&mut h);
    // J0 = GHASH_H(IV ‖ 0^64 ‖ [len(IV) in bits]_64), for IV ≠ 96 bits.
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

    /// Encrypts like WhatsApp (judging by what the references decrypt).
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
    fn expand_matches_crypt15_derive() {
        // The first block with info "backup encryption" is the crypt15 key.
        let mut out = [0u8; 32];
        expand(&ROOT, b"backup encryption", &mut out);
        assert_eq!(&out, key().aes_key());
    }

    #[test]
    fn media_with_trailing_tag() {
        let name = "00112233445566778899aabbccddeeff";
        let plain = b"\xFF\xD8\xFF\xE0 some arbitrary photo".repeat(100);
        let (mut ct, tag) = encrypt_media(name, &plain);
        ct.extend_from_slice(&tag);
        let m = decrypt_media(&key(), name, ct).unwrap();
        assert!(m.verified);
        assert_eq!(&*m.data, &plain);
    }

    #[test]
    fn media_without_tag_decrypts_unverified() {
        let name = "a1b2c3";
        let plain = b"content without tag".repeat(77);
        let (ct, _) = encrypt_media(name, &plain);
        let m = decrypt_media(&key(), name, ct).unwrap();
        assert!(!m.verified);
        assert_eq!(&*m.data, &plain, "the manual keystream must match the AES-GCM one");
    }

    #[test]
    fn metadata() {
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
    fn crypt14_key_is_rejected() {
        let data = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/key")).unwrap();
        let k = BackupKey::from_key_file(&data).unwrap();
        assert!(matches!(decrypt_media(&k, "aa", vec![0; 32]), Err(Error::InvalidKey(_))));
    }
}
