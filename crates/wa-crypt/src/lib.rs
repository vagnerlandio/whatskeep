//! Descriptografia de backups do WhatsApp (`.crypt14` e `.crypt15`).
//!
//! Formato (referência: wa-crypt-tools, ElDavoo):
//!
//! ```text
//! [varint: tamanho N][N bytes: protobuf BackupPrefix][ciphertext][tag GCM 16][md5 16]
//! ```
//!
//! * O `BackupPrefix` traz o IV de 16 bytes. O campo 2 (`.5`) é usado no crypt14
//!   e o campo 3 (`.1`) no crypt15.
//! * A cifra é AES-256-GCM com **nonce de 16 bytes**, sem AAD.
//! * O md5 final cobre o arquivo inteiro antes dele. Backups "multi-file"
//!   (stickers, wallpapers) não têm md5: terminam direto na tag.
//! * O payload decifrado normalmente vem comprimido em zlib (o `msgstore` é um SQLite).
//!
//! Garantias de segurança deste crate:
//! * Chaves e texto puro ficam em buffers [`zeroize::Zeroizing`], zerados no drop,
//!   inclusive quando o buffer é realocado durante a descompressão.
//! * Nenhum `Debug`, `Display` ou mensagem de erro expõe material de chave.
//! * Não há I/O: o chamador decide onde os bytes entram e saem.

mod decrypt;
mod error;
mod key;
mod prefix;

pub use decrypt::{decrypt, Decrypted, Payload};
pub use error::Error;
pub use key::{BackupKey, KeyKind};
pub use prefix::{parse_prefix, CryptVersion, Prefix};
