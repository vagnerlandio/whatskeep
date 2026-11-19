//! Arquivos e pastas do cofre só com permissão do dono (0600/0700 no Unix).

use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::Path;

pub(crate) fn create_private(path: &Path) -> io::Result<File> {
    let mut opts = OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut opts, 0o600);
    opts.open(path)
}

pub(crate) fn create_private_dir(path: &Path) -> io::Result<()> {
    fs::create_dir_all(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

#[cfg(unix)]
pub(crate) fn set_private(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
pub(crate) fn set_private(_path: &Path) -> io::Result<()> {
    Ok(())
}

/// Pede ao Spotlight (macOS) para não indexar a pasta. Os bancos já são
/// cifrados, mas assim nem os nomes de arquivo entram no índice.
pub(crate) fn mark_no_index(dir: &Path) -> io::Result<()> {
    create_private(&dir.join(".metadata_never_index")).map(drop)
}

/// Tira a pasta dos backups do Time Machine (macOS). É a mesma marca que o
/// `tmutil addexclusion` grava: o atributo estendido
/// `com.apple.metadata:com_apple_backup_excludeItem` com o plist binário da
/// string "com.apple.backupd". Acompanha a pasta se ela for movida.
#[cfg(target_os = "macos")]
pub(crate) fn exclude_from_backup(path: &Path) -> io::Result<()> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    const NAME: &[u8] = b"com.apple.metadata:com_apple_backup_excludeItem\0";
    let value = backupd_bplist();
    let p = CString::new(path.as_os_str().as_bytes()).map_err(io::Error::other)?;
    // SAFETY: ponteiros válidos durante a chamada; tamanhos corretos.
    let r = unsafe { libc::setxattr(p.as_ptr(), NAME.as_ptr().cast(), value.as_ptr().cast(), value.len(), 0, 0) };
    if r == 0 { Ok(()) } else { Err(io::Error::last_os_error()) }
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn exclude_from_backup(_path: &Path) -> io::Result<()> {
    Ok(())
}

/// Plist binário com uma única string ASCII "com.apple.backupd".
#[cfg(target_os = "macos")]
fn backupd_bplist() -> Vec<u8> {
    let s = b"com.apple.backupd";
    let mut v = b"bplist00".to_vec();
    v.extend_from_slice(&[0x5F, 0x10, s.len() as u8]); // string ASCII, tamanho em int de 1 byte
    v.extend_from_slice(s);
    let table = v.len() as u64;
    v.push(8); // tabela de offsets: o objeto 0 começa no byte 8
    v.extend_from_slice(&[0; 6]);
    v.extend_from_slice(&[1, 1]); // offsets e referências de 1 byte
    v.extend_from_slice(&1u64.to_be_bytes()); // 1 objeto
    v.extend_from_slice(&0u64.to_be_bytes()); // raiz = objeto 0
    v.extend_from_slice(&table.to_be_bytes());
    v
}
