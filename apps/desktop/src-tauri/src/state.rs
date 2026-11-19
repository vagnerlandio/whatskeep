//! Estado do cofre no processo e bloqueio automático por inatividade.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use rusqlite::Connection;
use tauri::{AppHandle, Emitter, Manager};
use wa_import::SourceApp;
use wa_vault::{MediaReader, Vault};

use crate::error::CmdError;

/// Tempo sem interação até travar o cofre.
pub const AUTOLOCK_AFTER: Duration = Duration::from_secs(5 * 60);
const AUTOLOCK_CHECK_EVERY: Duration = Duration::from_secs(5);

pub const EVENT_LOCKED: &str = "vault-locked";

/// Ordem de lock, para não haver deadlock: `conns`, `media`, `vault`.
pub struct AppState {
    pub vault_dir: PathBuf,
    /// Conexões só-leitura do visualizador, abertas sob demanda e fechadas ao travar.
    conns: Mutex<HashMap<SourceApp, Connection>>,
    /// Índices de mídia só-leitura (idem).
    media: Mutex<HashMap<SourceApp, MediaReader>>,
    /// Pedido de cancelamento da importação de mídias em andamento.
    pub media_cancel: AtomicBool,
    vault: Mutex<Option<Vault>>,
    last_activity: Mutex<Instant>,
    /// Importações em andamento. O autolock espera elas terminarem.
    busy: AtomicUsize,
    /// Senhas erradas seguidas (para atrasar tentativas pela UI).
    failed_unlocks: AtomicUsize,
    /// Sessão do Google Drive (só tokens de acesso, nunca o token mestre).
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

    /// Trava: fecha as conexões (o SQLCipher zera o cache de páginas) e a
    /// DEK é zerada no drop do `Vault`.
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

    /// Roda `f` com o índice de mídias de `app` (`None` se ainda não há mídias).
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
                // Só guarda quando existe: o índice pode ser criado depois.
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

    /// Espera antes de testar a senha: nada nas 3 primeiras tentativas, depois
    /// 1, 2, 4... até 30 s. Somado ao Argon2id, chutar senhas pela UI fica
    /// inviável. (Quem copiar o vault.json ainda pode atacar offline: por isso
    /// o Argon2id caro e a recomendação de uma senha longa.)
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

/// Thread que trava o cofre depois de [`AUTOLOCK_AFTER`] sem interação e avisa a UI.
pub fn spawn_autolock(app: AppHandle) {
    std::thread::Builder::new()
        .name("autolock".into())
        .spawn(move || loop {
            std::thread::sleep(AUTOLOCK_CHECK_EVERY);
            let state = app.state::<AppState>();
            if state.busy.load(Ordering::SeqCst) == 0 && state.idle_for() >= AUTOLOCK_AFTER && state.vault().is_some() {
                tracing::info!(inativo_s = state.idle_for().as_secs(), "cofre travado por inatividade");
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atraso_cresce_com_as_senhas_erradas() {
        let s = AppState::new(PathBuf::from("/nao/existe"));
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
