//! Estado do cofre no processo e bloqueio automático por inatividade.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use rusqlite::Connection;
use tauri::{AppHandle, Emitter, Manager};
use wa_import::SourceApp;
use wa_vault::Vault;

use crate::error::CmdError;

/// Tempo sem interação até travar o cofre.
pub const AUTOLOCK_AFTER: Duration = Duration::from_secs(5 * 60);
const AUTOLOCK_CHECK_EVERY: Duration = Duration::from_secs(5);

pub const EVENT_LOCKED: &str = "vault-locked";

/// Ordem de lock, para não haver deadlock: `conns` antes de `vault`.
pub struct AppState {
    pub vault_dir: PathBuf,
    /// Conexões só-leitura do visualizador, abertas sob demanda e fechadas ao travar.
    conns: Mutex<HashMap<SourceApp, Connection>>,
    vault: Mutex<Option<Vault>>,
    last_activity: Mutex<Instant>,
    /// Importações em andamento. O autolock espera elas terminarem.
    busy: AtomicUsize,
    /// Sessão do Google Drive (só tokens de acesso, nunca o token mestre).
    pub drive: crate::gdrive::DriveState,
}

impl AppState {
    pub fn new(vault_dir: PathBuf) -> Self {
        Self {
            vault_dir,
            conns: Mutex::new(HashMap::new()),
            vault: Mutex::new(None),
            last_activity: Mutex::new(Instant::now()),
            busy: AtomicUsize::new(0),
            drive: Default::default(),
        }
    }

    pub fn vault(&self) -> MutexGuard<'_, Option<Vault>> {
        self.vault.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn touch(&self) {
        *self.last_activity.lock().unwrap_or_else(|e| e.into_inner()) = Instant::now();
    }

    /// Trava: fecha as conexões (o SQLCipher zera o cache de páginas) e a
    /// DEK é zerada no drop do `Vault`.
    pub fn lock(&self) -> bool {
        let mut conns = self.conns();
        conns.clear();
        self.vault().take().is_some()
    }

    fn conns(&self) -> MutexGuard<'_, HashMap<SourceApp, Connection>> {
        self.conns.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Roda `f` na conexão só-leitura do banco de `app`, abrindo-a se preciso.
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

    /// Fecha a conexão de um banco (antes e depois de reimportá-lo).
    pub fn forget_store(&self, app: SourceApp) {
        self.conns().remove(&app);
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
            if state.busy.load(Ordering::SeqCst) == 0 && state.idle_for() >= AUTOLOCK_AFTER && state.vault().is_some() {
                lock_all(&app);
                let _ = app.emit(EVENT_LOCKED, "inactivity");
            }
            // Tokens de acesso valem 1 hora: a sessão do Drive some antes disso.
            state.drive.expire();
        })
        .expect("falha ao iniciar a thread de autolock");
}

/// Trava o cofre e encerra tudo o que depende dele: conexões, sessão do
/// Google Drive (tokens zerados) e a janela de login, se aberta.
pub fn lock_all(app: &AppHandle) {
    crate::gdrive::shutdown(app);
    app.state::<AppState>().lock();
}
