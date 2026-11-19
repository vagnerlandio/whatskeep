//! Estado do cofre no processo e bloqueio automático por inatividade.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter, Manager};
use wa_vault::Vault;

/// Tempo sem interação até travar o cofre.
pub const AUTOLOCK_AFTER: Duration = Duration::from_secs(5 * 60);
const AUTOLOCK_CHECK_EVERY: Duration = Duration::from_secs(5);

pub const EVENT_LOCKED: &str = "vault-locked";

pub struct AppState {
    pub vault_dir: PathBuf,
    vault: Mutex<Option<Vault>>,
    last_activity: Mutex<Instant>,
    /// Importações em andamento. O autolock espera elas terminarem.
    busy: AtomicUsize,
}

impl AppState {
    pub fn new(vault_dir: PathBuf) -> Self {
        Self {
            vault_dir,
            vault: Mutex::new(None),
            last_activity: Mutex::new(Instant::now()),
            busy: AtomicUsize::new(0),
        }
    }

    pub fn vault(&self) -> MutexGuard<'_, Option<Vault>> {
        self.vault.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn touch(&self) {
        *self.last_activity.lock().unwrap_or_else(|e| e.into_inner()) = Instant::now();
    }

    /// Trava: a DEK é zerada no drop do `Vault`.
    pub fn lock(&self) -> bool {
        self.vault().take().is_some()
    }

    /// Marca uma operação longa. O autolock não dispara enquanto o guard existir.
    pub fn busy(&self) -> BusyGuard<'_> {
        self.busy.fetch_add(1, Ordering::SeqCst);
        BusyGuard(self)
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

/// Thread que trava o cofre depois de [`AUTOLOCK_AFTER`] sem interação e avisa a UI.
pub fn spawn_autolock(app: AppHandle) {
    std::thread::Builder::new()
        .name("autolock".into())
        .spawn(move || loop {
            std::thread::sleep(AUTOLOCK_CHECK_EVERY);
            let state = app.state::<AppState>();
            if state.busy.load(Ordering::SeqCst) == 0 && state.idle_for() >= AUTOLOCK_AFTER && state.lock() {
                let _ = app.emit(EVENT_LOCKED, "inactivity");
            }
        })
        .expect("falha ao iniciar a thread de autolock");
}
