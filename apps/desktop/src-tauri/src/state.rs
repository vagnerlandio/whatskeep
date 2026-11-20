//! In-process vault state and automatic lock on inactivity.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use rusqlite::Connection;
use tauri::{AppHandle, Emitter, Manager};
use wk_import::SourceApp;
use wk_vault::{MediaReader, Vault};

use crate::error::CmdError;

/// Time without interaction until the vault locks.
pub const AUTOLOCK_AFTER: Duration = Duration::from_secs(5 * 60);
const AUTOLOCK_CHECK_EVERY: Duration = Duration::from_secs(5);

pub const EVENT_LOCKED: &str = "vault-locked";

/// Lock order, to avoid deadlocks: `conns`, `media`, `vault`.
pub struct AppState {
    pub vault_dir: PathBuf,
    /// The viewer's read-only connections, opened on demand and closed on lock.
    conns: Mutex<HashMap<SourceApp, Connection>>,
    /// Read-only media indexes (same).
    media: Mutex<HashMap<SourceApp, MediaReader>>,
    /// Cancellation request for the media import in progress.
    pub media_cancel: AtomicBool,
    vault: Mutex<Option<Vault>>,
    last_activity: Mutex<Instant>,
    /// Imports in progress. The autolock waits for them to finish.
    busy: AtomicUsize,
    /// An import or download job is running (only one at a time).
    job: AtomicBool,
    /// Consecutive wrong passwords (to slow down attempts through the UI).
    failed_unlocks: AtomicUsize,
    /// Google Drive session (access tokens only, never the master token).
    pub drive: crate::gdrive::DriveState,
    pub export: crate::export::ExportState,
}

impl AppState {
    pub fn new(vault_dir: PathBuf) -> Self {
        Self {
            vault_dir,
            conns: Mutex::new(HashMap::new()),
            media: Mutex::new(HashMap::new()),
            media_cancel: AtomicBool::new(false),
            vault: Mutex::new(None),
            last_activity: Mutex::new(Instant::now()),
            busy: AtomicUsize::new(0),
            job: AtomicBool::new(false),
            failed_unlocks: AtomicUsize::new(0),
            drive: Default::default(),
            export: Default::default(),
        }
    }

    pub fn vault(&self) -> MutexGuard<'_, Option<Vault>> {
        self.vault.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn touch(&self) {
        *self.last_activity.lock().unwrap_or_else(|e| e.into_inner()) = Instant::now();
    }

    /// Locks: closes the connections (SQLCipher zeroes the page cache) and the
    /// DEK is zeroed when the `Vault` is dropped.
    pub fn lock(&self) -> bool {
        self.media_cancel.store(true, Ordering::SeqCst);
        self.export.cancel.store(true, Ordering::SeqCst);
        let mut conns = self.conns();
        conns.clear();
        self.media_readers().clear();
        self.vault().take().is_some()
    }

    fn media_readers(&self) -> MutexGuard<'_, HashMap<SourceApp, MediaReader>> {
        self.media.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Runs `f` with the media index of `app` (`None` if there is no media yet).
    pub fn with_media<T>(
        &self,
        app: SourceApp,
        f: impl FnOnce(Option<&MediaReader>) -> Result<T, CmdError>,
    ) -> Result<T, CmdError> {
        let mut readers = self.media_readers();
        {
            let vault = self.vault();
            let Some(vault) = vault.as_ref() else {
                readers.clear();
                return Err(CmdError::locked());
            };
            if let std::collections::hash_map::Entry::Vacant(slot) = readers.entry(app) {
                // Only cached when it exists: the index may be created later.
                if let Some(r) = vault.media(app)?.reader()? {
                    slot.insert(r);
                }
            }
        }
        f(readers.get(&app))
    }

    fn conns(&self) -> MutexGuard<'_, HashMap<SourceApp, Connection>> {
        self.conns.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Runs `f` on the read-only connection to the database of `app`, opening it if needed.
    pub fn with_store<T>(
        &self,
        app: SourceApp,
        f: impl FnOnce(&Connection) -> Result<T, CmdError>,
    ) -> Result<T, CmdError> {
        let mut conns = self.conns();
        {
            let vault = self.vault();
            let Some(vault) = vault.as_ref() else {
                conns.clear();
                return Err(CmdError::locked());
            };
            if let std::collections::hash_map::Entry::Vacant(slot) = conns.entry(app) {
                let conn = vault
                    .store(app)?
                    .open_read_only()?
                    .ok_or_else(|| CmdError::new("no_store", "nenhum histórico importado deste app"))?;
                slot.insert(conn);
            }
        }
        f(&conns[&app])
    }

    /// Closes a database's connection (before and after reimporting it).
    pub fn forget_store(&self, app: SourceApp) {
        self.conns().remove(&app);
    }

    /// Closes an app's media index (before deleting it).
    pub fn forget_media(&self, app: SourceApp) {
        self.media_readers().remove(&app);
    }

    /// Marks a long operation. The autolock does not fire while the guard exists.
    pub fn busy(&self) -> BusyGuard<'_> {
        self.busy.fetch_add(1, Ordering::SeqCst);
        BusyGuard(self)
    }

    /// Starts an import or download job. Only one runs at a time (they write
    /// to the same databases); the autolock waits while it runs.
    pub fn start_job(&self) -> Result<JobGuard<'_>, CmdError> {
        if self.job.swap(true, Ordering::SeqCst) {
            return Err(CmdError::new("busy", "outra importação ou download já está em andamento"));
        }
        Ok(JobGuard { state: self, _busy: self.busy() })
    }

    /// Wait before checking the password: nothing on the first 3 attempts, then
    /// 1, 2, 4... up to 30 s. Together with Argon2id, guessing passwords through
    /// the UI becomes infeasible. (Whoever copies vault.json can still attack
    /// offline: hence the expensive Argon2id and the advice to use a long password.)
    pub fn unlock_delay(&self) -> Duration {
        let n = self.failed_unlocks.load(Ordering::SeqCst);
        if n < 3 { Duration::ZERO } else { Duration::from_secs((1u64 << (n - 3).min(5)).min(30)) }
    }

    pub fn unlock_result(&self, ok: bool) {
        if ok {
            self.failed_unlocks.store(0, Ordering::SeqCst);
        } else {
            self.failed_unlocks.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn idle_for(&self) -> Duration {
        self.last_activity.lock().unwrap_or_else(|e| e.into_inner()).elapsed()
    }
}

pub struct BusyGuard<'a>(&'a AppState);

impl Drop for BusyGuard<'_> {
    fn drop(&mut self) {
        self.0.touch();
        self.0.busy.fetch_sub(1, Ordering::SeqCst);
    }
}

pub struct JobGuard<'a> {
    state: &'a AppState,
    _busy: BusyGuard<'a>,
}

impl Drop for JobGuard<'_> {
    fn drop(&mut self) {
        self.state.job.store(false, Ordering::SeqCst);
    }
}

/// Thread that locks the vault after [`AUTOLOCK_AFTER`] without interaction and notifies the UI.
pub fn spawn_autolock(app: AppHandle) {
    std::thread::Builder::new()
        .name("autolock".into())
        .spawn(move || loop {
            std::thread::sleep(AUTOLOCK_CHECK_EVERY);
            let state = app.state::<AppState>();
            if state.busy.load(Ordering::SeqCst) == 0 && state.idle_for() >= AUTOLOCK_AFTER && state.vault().is_some() {
                tracing::info!(idle_s = state.idle_for().as_secs(), "vault locked due to inactivity");
                lock_all(&app);
                let _ = app.emit(EVENT_LOCKED, "inactivity");
            }
            // Access tokens are valid for 1 hour: the Drive session goes away before that.
            state.drive.expire();
        })
        .expect("failed to start the autolock thread");
}

/// Locks the vault and shuts down everything that depends on it: connections,
/// the Google Drive session (tokens zeroed) and the login window, if open.
pub fn lock_all(app: &AppHandle) {
    crate::gdrive::shutdown(app);
    app.state::<AppState>().lock();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delay_grows_with_wrong_passwords() {
        let s = AppState::new(PathBuf::from("/does/not/exist"));
        let delays: Vec<u64> = (0..10)
            .map(|_| {
                let d = s.unlock_delay().as_secs();
                s.unlock_result(false);
                d
            })
            .collect();
        assert_eq!(delays, [0, 0, 0, 1, 2, 4, 8, 16, 30, 30]);
        s.unlock_result(true);
        assert_eq!(s.unlock_delay(), Duration::ZERO);
    }
}
