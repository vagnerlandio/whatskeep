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
