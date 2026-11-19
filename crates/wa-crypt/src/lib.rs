//! Decryption of WhatsApp backups (`.crypt14` and `.crypt15`).
//!
//! Format (reference: wa-crypt-tools, ElDavoo):
//!
//! ```text
//! [varint: size N][N bytes: protobuf BackupPrefix][ciphertext][GCM tag 16][md5 16]
//! ```
//!
//! * The `BackupPrefix` carries the 16-byte IV. Field 2 (`.5`) is used by crypt14
//!   and field 3 (`.1`) by crypt15.
//! * The cipher is AES-256-GCM with a **16-byte nonce**, no AAD.
//! * The trailing md5 covers the whole file before it. "Multi-file" backups
//!   (stickers, wallpapers) have no md5: they end right at the tag.
//! * The decrypted payload is usually zlib-compressed (the `msgstore` is a SQLite DB).
//!
//! Security guarantees of this crate:
//! * Keys and plaintext live in [`zeroize::Zeroizing`] buffers, zeroed on drop,
//!   including when the buffer is reallocated during decompression.
//! * No `Debug`, `Display` or error message exposes key material.
//! * There is no I/O: the caller decides where the bytes come in and go out.

mod decrypt;
mod error;
mod key;
pub mod mcrypt1;
mod prefix;

pub use decrypt::{decrypt, Decrypted, Payload};
pub use error::Error;
pub use key::{BackupKey, KeyKind};
pub use prefix::{parse_prefix, CryptVersion, Prefix};
