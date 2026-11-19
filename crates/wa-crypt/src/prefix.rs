//! Minimal reader for the `BackupPrefix` protobuf, without codegen.
//!
//! ```text
//! BackupPrefix { 2: C14Cipher { 5: bytes encryption_iv }, 3: C15IV { 1: bytes encryption_iv }, ... }
//! ```

use crate::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CryptVersion {
    Crypt14,
    Crypt15,
}

#[derive(Clone, Debug)]
pub struct Prefix {
    pub version: CryptVersion,
    pub iv: [u8; 16],
    /// Bytes of the varint + protobuf. The ciphertext starts here.
    pub header_len: usize,
}

/// Reads the header of a `.crypt14`/`.crypt15` file.
pub fn parse_prefix(data: &[u8]) -> Result<Prefix, Error> {
    let mut pos = 0;
    // wa-crypt-tools limits the size varint to 5 bytes.
    let size = read_varint(data, &mut pos, 5)? as usize;
    let end = pos.checked_add(size).ok_or(Error::Truncated)?;
    let proto = data.get(pos..end).ok_or(Error::Truncated)?;

    let iv15 = nested_bytes(proto, 3, 1)?;
    let iv14 = nested_bytes(proto, 2, 5)?;
    let (version, iv) = match (iv15, iv14) {
        (Some(iv), _) => (CryptVersion::Crypt15, iv),
        (None, Some(iv)) => (CryptVersion::Crypt14, iv),
        (None, None) => return Err(Error::Unsupported),
    };
    let iv: [u8; 16] = iv
        .try_into()
        .map_err(|_| Error::Header("IV deve ter 16 bytes"))?;
    Ok(Prefix {
        version,
        iv,
        header_len: end,
    })
}

/// Returns `msg.outer.inner` as bytes (empty counts as absent).
fn nested_bytes(msg: &[u8], outer: u64, inner: u64) -> Result<Option<&[u8]>, Error> {
    let Some(sub) = last_bytes_field(msg, outer)? else {
        return Ok(None);
    };
    Ok(last_bytes_field(sub, inner)?.filter(|v| !v.is_empty()))
}

/// In protobuf, the last occurrence of a scalar field is the one that counts.
fn last_bytes_field(msg: &[u8], field: u64) -> Result<Option<&[u8]>, Error> {
    let mut pos = 0;
    let mut found = None;
    while pos < msg.len() {
        let tag = read_varint(msg, &mut pos, 10)?;
        let (num, wire) = (tag >> 3, tag & 7);
        match wire {
            0 => {
                read_varint(msg, &mut pos, 10)?;
            }
            1 => pos = advance(msg, pos, 8)?,
            2 => {
                let len = read_varint(msg, &mut pos, 10)? as usize;
                let start = pos;
                pos = advance(msg, pos, len)?;
                if num == field {
                    found = Some(&msg[start..pos]);
                }
            }
            5 => pos = advance(msg, pos, 4)?,
            _ => return Err(Error::Header("tipo de campo protobuf não suportado")),
        }
    }
    Ok(found)
}

fn advance(buf: &[u8], pos: usize, n: usize) -> Result<usize, Error> {
    let end = pos.checked_add(n).ok_or(Error::Truncated)?;
    if end > buf.len() {
        return Err(Error::Truncated);
    }
    Ok(end)
}

fn read_varint(buf: &[u8], pos: &mut usize, max_bytes: usize) -> Result<u64, Error> {
    let mut value = 0u64;
    for i in 0..max_bytes {
        let b = *buf.get(*pos).ok_or(Error::Truncated)?;
        *pos += 1;
        value |= u64::from(b & 0x7F) << (7 * i);
        if b & 0x80 == 0 {
            return Ok(value);
        }
    }
    Err(Error::Header("varint longo demais"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimal_crypt15() {
        // size=20 | field 3 (len 18) { field 1 (len 16) IV }
        let mut d = vec![20, 0x1A, 18, 0x0A, 16];
        d.extend_from_slice(&[7u8; 16]);
        d.extend_from_slice(b"ciphertext");
        let p = parse_prefix(&d).unwrap();
        assert_eq!(p.version, CryptVersion::Crypt15);
        assert_eq!(p.iv, [7u8; 16]);
        assert_eq!(p.header_len, 21);
    }

    #[test]
    fn missing_iv_is_unsupported() {
        // crypt12 starts with 00 01 → varint 0, empty protobuf
        assert!(matches!(parse_prefix(&[0, 1, 2, 3]), Err(Error::Unsupported)));
    }

    #[test]
    fn truncated() {
        assert!(matches!(parse_prefix(&[50, 0x1A]), Err(Error::Truncated)));
        assert!(matches!(parse_prefix(&[]), Err(Error::Truncated)));
        assert!(matches!(parse_prefix(&[0x80, 0x80, 0x80, 0x80, 0x80, 0x01]), Err(Error::Header(_))));
    }

    #[test]
    fn iv_with_wrong_length() {
        let d = [7, 0x1A, 5, 0x0A, 3, 1, 2, 3];
        assert!(matches!(parse_prefix(&d), Err(Error::Header(_))));
    }
}
