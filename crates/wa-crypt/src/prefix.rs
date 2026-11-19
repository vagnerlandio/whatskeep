//! Leitura mínima do protobuf `BackupPrefix`, sem codegen.
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
    /// Bytes do varint + protobuf. O ciphertext começa aqui.
    pub header_len: usize,
}

/// Lê o header de um arquivo `.crypt14`/`.crypt15`.
pub fn parse_prefix(data: &[u8]) -> Result<Prefix, Error> {
    let mut pos = 0;
    // O wa-crypt-tools limita o varint do tamanho a 5 bytes.
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

/// Retorna `msg.outer.inner` como bytes (vazio conta como ausente).
fn nested_bytes(msg: &[u8], outer: u64, inner: u64) -> Result<Option<&[u8]>, Error> {
    let Some(sub) = last_bytes_field(msg, outer)? else {
        return Ok(None);
    };
    Ok(last_bytes_field(sub, inner)?.filter(|v| !v.is_empty()))
}

/// No protobuf, a última ocorrência de um campo escalar é a que vale.
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
    fn crypt15_minimo() {
        // tamanho=20 | campo 3 (len 18) { campo 1 (len 16) IV }
        let mut d = vec![20, 0x1A, 18, 0x0A, 16];
        d.extend_from_slice(&[7u8; 16]);
        d.extend_from_slice(b"ciphertext");
        let p = parse_prefix(&d).unwrap();
        assert_eq!(p.version, CryptVersion::Crypt15);
        assert_eq!(p.iv, [7u8; 16]);
        assert_eq!(p.header_len, 21);
    }

    #[test]
    fn sem_iv_e_nao_suportado() {
        // crypt12 começa com 00 01 → varint 0, protobuf vazio
        assert!(matches!(parse_prefix(&[0, 1, 2, 3]), Err(Error::Unsupported)));
    }

    #[test]
    fn truncado() {
        assert!(matches!(parse_prefix(&[50, 0x1A]), Err(Error::Truncated)));
        assert!(matches!(parse_prefix(&[]), Err(Error::Truncated)));
        assert!(matches!(parse_prefix(&[0x80, 0x80, 0x80, 0x80, 0x80, 0x01]), Err(Error::Header(_))));
    }

    #[test]
    fn iv_de_tamanho_errado() {
        let d = [7, 0x1A, 5, 0x0A, 3, 1, 2, 3];
        assert!(matches!(parse_prefix(&d), Err(Error::Header(_))));
    }
}
