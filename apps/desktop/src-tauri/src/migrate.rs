//! Moves the app data folder from the old bundle identifier to the current one.
//!
//! The data folder is named after the bundle identifier
//! (`~/Library/Application Support/<identifier>/` on macOS). The app was
//! first published as `com.local.wahistorico` and is now `com.whatskeep.desktop`,
//! so without this the vault would seem to disappear.
//!
//! * Only renames (same parent folder, same volume): nothing is copied or
//!   rewritten, so the vault keeps its permissions and its Time Machine
//!   exclusion, and the move is atomic.
//! * Never overwrites: if the new folder already has a vault, the old one is
//!   left where it is (and a warning is logged).
//! * If the move fails, the app keeps using the old folder, so the user never
//!   ends up with an apparently empty vault.

use std::path::{Path, PathBuf};

/// Previous bundle identifiers, newest first.
pub const LEGACY_IDENTIFIERS: &[&str] = &["com.local.wahistorico"];

/// Entries the app keeps in its data folder.
const ENTRIES: &[&str] = &["vault", "settings.json"];

#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Nothing to migrate.
    NotNeeded,
    /// The whole old folder was renamed to the new one.
    Moved,
    /// The new folder already existed (without a vault); entries were moved into it.
    Merged(Vec<&'static str>),
    /// Both folders have a vault: nothing was touched.
    BothHaveVault,
    /// The move failed: keep using the old folder.
    Failed(String),
}

/// Migrates `legacy` into `current` if needed. Returns the folder the app must
/// use and what happened.
pub fn migrate(current: &Path, legacy: &Path) -> (PathBuf, Outcome) {
    if !legacy.is_dir() {
        return (current.to_owned(), Outcome::NotNeeded);
    }
    let legacy_has_vault = legacy.join("vault").join("vault.json").is_file();
    let current_has_vault = current.join("vault").join("vault.json").is_file();

    if current_has_vault {
        return if legacy_has_vault {
            (current.to_owned(), Outcome::BothHaveVault)
        } else {
            (current.to_owned(), Outcome::NotNeeded)
        };
    }

    if !current.exists() {
        if let Some(parent) = current.parent()
            && let Err(e) = std::fs::create_dir_all(parent)
        {
            return (legacy.to_owned(), Outcome::Failed(e.to_string()));
        }
        return match std::fs::rename(legacy, current) {
            Ok(()) => (current.to_owned(), Outcome::Moved),
            Err(e) => (legacy.to_owned(), Outcome::Failed(e.to_string())),
        };
    }

    // The new folder exists but has no vault (e.g. only settings were written
    // there): move each known entry that is missing from it.
    let mut moved = Vec::new();
    for entry in ENTRIES {
        let (from, to) = (legacy.join(entry), current.join(entry));
        if from.exists() && !to.exists() {
            if let Err(e) = std::fs::rename(&from, &to) {
                // A vault that could not be moved stays usable in the old place.
                if *entry == "vault" {
                    return (legacy.to_owned(), Outcome::Failed(e.to_string()));
                }
                continue;
            }
            moved.push(*entry);
        }
    }
    // Removes the old folder only if it ended up empty.
    let _ = std::fs::remove_dir(legacy);
    (current.to_owned(), Outcome::Merged(moved))
}

/// Runs the migration for every legacy identifier and logs the result.
pub fn run(current: &Path) -> PathBuf {
    let Some(parent) = current.parent() else { return current.to_owned() };
    let mut dir = current.to_owned();
    for id in LEGACY_IDENTIFIERS {
        let (used, outcome) = migrate(current, &parent.join(id));
        match &outcome {
            Outcome::NotNeeded => {}
            Outcome::Moved | Outcome::Merged(_) => {
                tracing::info!(from = id, outcome = ?outcome, "data folder migrated to the new identifier")
            }
            Outcome::BothHaveVault => {
                tracing::warn!(legacy = id, "a vault exists under both identifiers; using the new one, the old one was left untouched")
            }
            Outcome::Failed(e) => tracing::warn!(legacy = id, "data folder migration failed, using the old folder: {e}"),
        }
        if used != current {
            dir = used;
            break;
        }
    }
    dir
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn vault(dir: &Path, marker: &str) {
        fs::create_dir_all(dir.join("vault/stores")).unwrap();
        fs::write(dir.join("vault/vault.json"), marker).unwrap();
        fs::write(dir.join("vault/stores/whatsapp.db"), b"encrypted").unwrap();
    }

    fn read_marker(dir: &Path) -> String {
        fs::read_to_string(dir.join("vault/vault.json")).unwrap()
    }

    #[test]
    fn moves_the_whole_legacy_folder() {
        let root = tempfile::tempdir().unwrap();
        let (current, legacy) = (root.path().join("com.whatskeep.desktop"), root.path().join("com.local.wahistorico"));
        vault(&legacy, "old");
        fs::write(legacy.join("settings.json"), r#"{"language":"en"}"#).unwrap();

        let (used, outcome) = migrate(&current, &legacy);
        assert_eq!(outcome, Outcome::Moved);
        assert_eq!(used, current);
        assert_eq!(read_marker(&current), "old");
        assert!(current.join("vault/stores/whatsapp.db").is_file());
        assert!(current.join("settings.json").is_file());
        assert!(!legacy.exists());

        // Second run: nothing left to do.
        assert_eq!(migrate(&current, &legacy), (current.clone(), Outcome::NotNeeded));
    }

    #[cfg(unix)]
    #[test]
    fn keeps_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        let (current, legacy) = (root.path().join("new"), root.path().join("old"));
        vault(&legacy, "old");
        fs::set_permissions(legacy.join("vault"), fs::Permissions::from_mode(0o700)).unwrap();
        migrate(&current, &legacy);
        let mode = fs::metadata(current.join("vault")).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700);
    }

    #[test]
    fn merges_into_an_existing_folder_without_vault() {
        let root = tempfile::tempdir().unwrap();
        let (current, legacy) = (root.path().join("new"), root.path().join("old"));
        vault(&legacy, "old");
        fs::write(legacy.join("settings.json"), "old settings").unwrap();
        fs::create_dir_all(&current).unwrap();
        fs::write(current.join("settings.json"), "new settings").unwrap();

        let (used, outcome) = migrate(&current, &legacy);
        assert_eq!(used, current);
        assert_eq!(outcome, Outcome::Merged(vec!["vault"]));
        assert_eq!(read_marker(&current), "old");
        // The newer settings win; the old file stays in the old folder.
        assert_eq!(fs::read_to_string(current.join("settings.json")).unwrap(), "new settings");
        assert!(legacy.join("settings.json").is_file());
    }

    #[test]
    fn never_overwrites_an_existing_vault() {
        let root = tempfile::tempdir().unwrap();
        let (current, legacy) = (root.path().join("new"), root.path().join("old"));
        vault(&legacy, "old");
        vault(&current, "new");

        let (used, outcome) = migrate(&current, &legacy);
        assert_eq!((used, outcome), (current.clone(), Outcome::BothHaveVault));
        assert_eq!(read_marker(&current), "new");
        assert_eq!(read_marker(&legacy), "old");
    }

    #[test]
    fn nothing_to_do_without_legacy_folder() {
        let root = tempfile::tempdir().unwrap();
        let current = root.path().join("new");
        assert_eq!(migrate(&current, &root.path().join("old")), (current.clone(), Outcome::NotNeeded));
        assert!(!current.exists());
    }

    #[test]
    fn run_uses_the_migrated_folder() {
        let root = tempfile::tempdir().unwrap();
        let current = root.path().join("com.whatskeep.desktop");
        vault(&root.path().join(LEGACY_IDENTIFIERS[0]), "old");
        assert_eq!(run(&current), current);
        assert_eq!(read_marker(&current), "old");
    }
}
