//! Download do backup direto do Google Drive (fluxo não oficial).
//!
//! 1. Uma janela **anônima** (sem cookies persistentes) abre o login
//!    `EmbeddedSetup` do Google. Ela não tem nenhuma permissão de IPC (as
//!    capabilities só valem para a janela `main`) e só navega por domínios do
//!    Google.
//! 2. O cookie `oauth_token` é lido pelo backend assim que o login termina, e a
//!    janela é destruída (junto com a sessão do navegador).
//! 3. O cookie vira token mestre e, na hora, tokens de acesso `drive.appdata`
//!    (1 h) para o WhatsApp e o Business. O token mestre é zerado em seguida.
//! 4. Os tokens de acesso ficam só em memória até o download terminar, o
//!    usuário cancelar, o cofre travar ou passarem 50 minutos.
//!
//! O backup baixado continua cifrado (crypt14/15) e vai direto da memória para
//! a descriptografia e a importação: nada é gravado em disco.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, Url, WebviewUrl, WebviewWindowBuilder};
use wa_gdrive::{Api, Backup, BackupFile, DriveApp, Endpoints, OauthCookie, Session};
use wa_import::SourceApp;
use zeroize::Zeroizing;

use crate::commands::{ImportSummary, KeyInput, import_encrypted, progress, source_app};
use crate::error::CmdError;
use crate::media_import::{MediaSummary, Plan, already_stored, store_one};
use crate::state::AppState;

const LOGIN_WINDOW: &str = "google-login";
const LOGIN_TIMEOUT: Duration = Duration::from_secs(15 * 60);
const SESSION_TTL: Duration = Duration::from_secs(50 * 60);
const EVENT_DOWNLOAD: &str = "gdrive-progress";

/// Domínios por onde o login do Google passa. Qualquer outra navegação na
/// janela de login é bloqueada.
const ALLOWED_DOMAINS: &[&str] = &["gstatic.com", "googleusercontent.com", "googleapis.com", "youtube.com"];

#[derive(Default)]
pub struct DriveState {
    session: Mutex<Option<Logged>>,
    /// Pedido de cancelamento do login ou do download em andamento.
    cancel: Arc<AtomicBool>,
}

struct Logged {
    session: Session,
    backups: Vec<Found>,
    at: Instant,
}

#[derive(Clone)]
struct Found {
    backup: Backup,
    /// Apps cujo token enxerga este backup.
    apps: Vec<DriveApp>,
}

impl DriveState {
    fn session(&self) -> MutexGuard<'_, Option<Logged>> {
        self.session.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Descarta a sessão (os tokens são zerados no drop).
    pub fn clear(&self) {
        self.session().take();
    }

    pub fn expire(&self) {
        let mut s = self.session();
        if s.as_ref().is_some_and(|l| l.at.elapsed() >= SESSION_TTL) {
            s.take();
        }
    }
}

/// Cancela o que estiver em andamento, descarta os tokens e fecha a janela de login.
pub fn shutdown(app: &AppHandle) {
    let state = app.state::<AppState>();
    state.drive.cancel.store(true, Ordering::SeqCst);
    state.drive.clear();
    if let Some(w) = app.get_webview_window(LOGIN_WINDOW) {
        let _ = w.destroy();
    }
}

fn allowed_navigation(url: &Url) -> bool {
    if url.scheme() != "https" {
        return false;
    }
    let Some(host) = url.host_str() else { return false };
    if ALLOWED_DOMAINS.iter().any(|d| host == *d || host.ends_with(&format!(".{d}"))) {
        return true;
    }
    // google.com, google.de, accounts.google.com.br, google.co.uk. Só esses
    // formatos de sufixo: algo como `google.us.com` é registrável por qualquer um.
    let labels: Vec<&str> = host.split('.').collect();
    labels.iter().position(|l| *l == "google").is_some_and(|i| match &labels[i + 1..] {
        ["com"] => true,
        [cc] => cc.len() == 2,
        [sld, cc] => (*sld == "com" || *sld == "co") && cc.len() == 2,
        _ => false,
    })
}

fn check_email(email: &str) -> Result<(), CmdError> {
    let ok = email.len() <= 320 && email.split_once('@').is_some_and(|(u, d)| !u.is_empty() && d.contains('.'));
    if ok { Ok(()) } else { Err(CmdError::new("invalid_argument", "e-mail inválido")) }
}

fn require_unlocked(state: &AppState) -> Result<(), CmdError> {
    state.vault().as_ref().map(|_| ()).ok_or_else(CmdError::locked)
}

impl From<wa_gdrive::Error> for CmdError {
    fn from(e: wa_gdrive::Error) -> Self {
        use wa_gdrive::Error as E;
        let code = match &e {
            E::BadAuthentication | E::NeedsBrowser | E::Google(_) => "google_auth",
            E::TokenExpired => "google_expired",
            E::Cancelled => "cancelled",
            E::Checksum | E::Incomplete => "download_corrupt",
            E::Network(_) | E::Tls(_) => "network",
            _ => "google",
        };
        CmdError::new(code, e.to_string())
    }
}

// ---------------------------------------------------------------- login

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupDto {
    /// Índice na sessão. O nome real do backup (`clients/wa/backups/<telefone>`)
    /// não volta do webview: o download usa este índice.
    id: usize,
    #[serde(flatten)]
    backup: BackupView,
    apps: Vec<DriveApp>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BackupView {
    phone: Option<String>,
    update_time: Option<String>,
    size_bytes: Option<u64>,
    uploading: bool,
    encrypted: Option<bool>,
    chat_db_size: Option<u64>,
    media_size: Option<u64>,
    messages: Option<u64>,
    app_version: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginResult {
    backups: Vec<BackupDto>,
    /// Apps que o Google recusou (ex.: conta sem WhatsApp Business).
    refused: Vec<DriveApp>,
}

/// Abre a janela de login do Google e espera o usuário entrar.
#[tauri::command]
pub async fn gdrive_login(app: AppHandle, email: String) -> Result<LoginResult, CmdError> {
    check_email(&email)?;
    require_unlocked(&app.state::<AppState>())?;
    if let Some(w) = app.get_webview_window(LOGIN_WINDOW) {
        let _ = w.set_focus();
        return Err(CmdError::new("busy", "a janela de login já está aberta"));
    }
    let state = app.state::<AppState>();
    state.drive.clear();
    state.drive.cancel.store(false, Ordering::SeqCst);

    let login_url: Url = wa_gdrive::LOGIN_URL.parse().expect("URL fixa válida");
    WebviewWindowBuilder::new(&app, LOGIN_WINDOW, WebviewUrl::External(login_url))
        .title("Entrar no Google")
        .inner_size(480.0, 720.0)
        .incognito(true)
        .on_navigation(|url| {
            let ok = allowed_navigation(url);
            // Só host e caminho (sem query): nada de e-mail ou códigos.
            let shown = format!("{}{}", url.host_str().unwrap_or(""), wa_gdrive::trace::scrub_digits(url.path()));
            if ok {
                tracing::debug!("login Google: navegou para {shown}");
            } else {
                tracing::warn!(esquema = url.scheme(), "login Google: navegação BLOQUEADA para {shown}");
            }
            ok
        })
        .build()
        .map_err(|e| CmdError::new("internal", format!("não foi possível abrir a janela de login: {e}")))?;

    let app2 = app.clone();
    let cookie = crate::commands::blocking("gdrive_login_aguardando_cookie", move || wait_for_cookie(&app2)).await?;
    finish_login(app, email, cookie).await
}

/// Plano B: o usuário fez login no navegador e colou o cookie `oauth_token`.
#[tauri::command]
pub async fn gdrive_login_manual(app: AppHandle, email: String, oauth_token: String) -> Result<LoginResult, CmdError> {
    let cookie = OauthCookie::new(oauth_token.trim().to_owned());
    drop(Zeroizing::new(oauth_token));
    check_email(&email)?;
    require_unlocked(&app.state::<AppState>())?;
    app.state::<AppState>().drive.clear();
    if cookie.expose().is_empty() {
        return Err(CmdError::new("invalid_argument", "cole o valor do cookie oauth_token"));
    }
    finish_login(app, email, cookie).await
}

/// Lê os cookies da janela até aparecer o `oauth_token` (ou a janela fechar).
fn wait_for_cookie(app: &AppHandle) -> Result<OauthCookie, CmdError> {
    let accounts: Url = "https://accounts.google.com/".parse().expect("URL fixa válida");
    let cancel = app.state::<AppState>().drive.cancel.clone();
    let started = Instant::now();
    let mut last_names: Vec<String> = Vec::new();
    tracing::info!("login Google: janela aberta, aguardando o cookie oauth_token");
    loop {
        std::thread::sleep(Duration::from_millis(700));
        let Some(window) = app.get_webview_window(LOGIN_WINDOW) else {
            return Err(CmdError::new("cancelled", "login cancelado"));
        };
        if cancel.load(Ordering::SeqCst) || started.elapsed() > LOGIN_TIMEOUT {
            let _ = window.destroy();
            return Err(CmdError::new("cancelled", "login cancelado"));
        }
        let cookies = match window.cookies_for_url(accounts.clone()) {
            Ok(c) => c,
            Err(e) => {
                tracing::debug!("login Google: não foi possível ler cookies: {e}");
                continue;
            }
        };
        // Só os nomes dos cookies (nunca os valores), quando mudam.
        let mut names: Vec<String> = cookies.iter().map(|c| c.name().to_owned()).collect();
        names.sort();
        if names != last_names {
            tracing::debug!(cookies = ?names, "login Google: cookies de accounts.google.com");
            last_names = names;
        }
        if let Some(c) = cookies.iter().find(|c| c.name() == wa_gdrive::LOGIN_COOKIE && !c.value().is_empty()) {
            let cookie = OauthCookie::new(c.value().to_owned());
            drop(cookies);
            tracing::info!(s = started.elapsed().as_secs(), "login Google: cookie oauth_token capturado");
            // Fecha a janela: a sessão anônima do navegador some junto.
            let _ = window.destroy();
            return Ok(cookie);
        }
    }
}

async fn finish_login(app: AppHandle, email: String, cookie: OauthCookie) -> Result<LoginResult, CmdError> {
    crate::commands::blocking("finish_login", move || {
        let ep = Endpoints::default();
        let session = wa_gdrive::login(&ep, &email, cookie)?;

        let mut found: Vec<Found> = Vec::new();
        let mut errors = Vec::new();
        for (drive_app, token) in &session.tokens {
            match Api::new(&ep, token).and_then(|api| api.backups()) {
                Ok(list) => {
                    for b in list {
                        match found.iter_mut().find(|f| f.backup.name == b.name) {
                            Some(f) => f.apps.push(*drive_app),
                            None => found.push(Found { backup: b, apps: vec![*drive_app] }),
                        }
                    }
                }
                Err(e) => errors.push(e),
            }
        }
        for e in &errors {
            tracing::warn!("listar backups falhou: {e}");
        }
        tracing::info!(
            backups = found.len(),
            apps_com_token = ?session.tokens.iter().map(|(a, _)| a.package()).collect::<Vec<_>>(),
            apps_recusados = ?session.refused.iter().map(|(a, e)| format!("{}: {e}", a.package())).collect::<Vec<_>>(),
            "login Google concluído"
        );
        for (i, f) in found.iter().enumerate() {
            let b = &f.backup;
            tracing::info!(
                id = i,
                apps = ?f.apps.iter().map(|a| a.package()).collect::<Vec<_>>(),
                atualizado = ?b.update_time,
                tamanho = ?b.size_bytes.map(crate::diag::mb),
                banco = ?b.chat_db_size.map(crate::diag::mb),
                midias = ?b.media_size.map(crate::diag::mb),
                mensagens = ?b.messages,
                e2e = ?b.encrypted,
                enviando = b.uploading,
                versao_app = ?b.app_version,
                "backup encontrado"
            );
        }
        if found.is_empty()
            && let Some(e) = errors.into_iter().next() {
                return Err(e.into());
            }

        let refused = session.refused.iter().map(|(a, _)| *a).collect();
        let dto = found
            .iter()
            .enumerate()
            .map(|(id, f)| BackupDto {
                id,
                apps: f.apps.clone(),
                backup: BackupView {
                    phone: f.backup.phone.clone(),
                    update_time: f.backup.update_time.clone(),
                    size_bytes: f.backup.size_bytes,
                    uploading: f.backup.uploading,
                    encrypted: f.backup.encrypted,
                    chat_db_size: f.backup.chat_db_size,
                    media_size: f.backup.media_size,
                    messages: f.backup.messages,
                    app_version: f.backup.app_version.clone(),
                },
            })
            .collect();

        let state = app.state::<AppState>();
        // O cofre pode ter travado durante o login: aí os tokens nem ficam.
        require_unlocked(&state)?;
        *state.drive.session() = Some(Logged { session, backups: found, at: Instant::now() });
        Ok(LoginResult { backups: dto, refused })
    })
    .await
}

/// Descarta os tokens do Google e fecha a janela de login.
#[tauri::command]
pub fn gdrive_cancel(app: AppHandle) {
    shutdown(&app);
}

// ---------------------------------------------------------------- download

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DownloadProgress {
    downloaded: u64,
    total: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveResult {
    summary: ImportSummary,
    media: Option<MediaSummary>,
}

/// Baixa o msgstore do backup escolhido, descriptografa e importa; depois,
/// se pedido, as mídias. Ao terminar (com sucesso ou não), os tokens do Google
/// são descartados.
#[tauri::command]
pub async fn gdrive_download(
    app: AppHandle,
    backup_id: usize,
    key: KeyInput,
    source_app: String,
    include_media: bool,
) -> Result<DriveResult, CmdError> {
    crate::commands::blocking("gdrive_download", move || {
        let state = app.state::<AppState>();
        let _busy = state.busy();
        let target: SourceApp = self::source_app(&source_app)?;
        // Chave errada ou malformada falha antes de baixar qualquer coisa.
        let key = key.load()?;
        require_unlocked(&state)?;

        // A sessão sai do estado: os tokens morrem no fim deste comando.
        let logged = state
            .drive
            .session()
            .take()
            .ok_or_else(|| CmdError::new("google_expired", "a sessão do Google expirou; entre de novo"))?;
        state.drive.cancel.store(false, Ordering::SeqCst);
        state.media_cancel.store(false, Ordering::SeqCst);
        let found = logged
            .backups
            .get(backup_id)
            .cloned()
            .ok_or_else(|| CmdError::new("invalid_argument", "backup desconhecido"))?;
        let token = logged
            .session
            .tokens
            .iter()
            .find(|(a, _)| found.apps.contains(a))
            .map(|(_, t)| t)
            .ok_or_else(|| CmdError::new("google_expired", "a sessão do Google expirou; entre de novo"))?;

        progress(&app, "downloading");
        let cancel = state.drive.cancel.clone();
        let ep = Endpoints::default();
        let downloaded = (|| -> Result<(Vec<u8>, Vec<BackupFile>), CmdError> {
            let api = Api::new(&ep, token)?;
            let files = api.files(&found.backup.name)?;
            log_files(&files);
            let db = wa_gdrive::find_msgstore(&files).ok_or_else(|| {
                CmdError::new("not_msgstore", "este backup não tem o banco de mensagens (msgstore) no Google Drive")
            })?;
            tracing::info!(arquivo = db.path, tamanho = %crate::diag::mb(db.size_bytes), md5 = db.md5.is_some(), "baixando o banco de mensagens");
            let mut next_log = 0u64;
            let bytes = api.download(db, &cancel, |downloaded, total| {
                if total > 0 && downloaded * 10 / total >= next_log {
                    tracing::debug!(baixado = %crate::diag::mb(downloaded), total = %crate::diag::mb(total), "download {}%", downloaded * 100 / total);
                    next_log = downloaded * 10 / total + 1;
                }
                let _ = app.emit(EVENT_DOWNLOAD, DownloadProgress { downloaded, total });
            })?;
            Ok((bytes, files))
        })();
        let (bytes, files) = match downloaded {
            Ok(v) => v,
            // Falha de rede ou arquivo corrompido: a sessão volta para tentar
            // de novo sem novo login (ela ainda expira em SESSION_TTL).
            Err(e) if matches!(e.code, "network" | "download_corrupt") && state.vault().is_some() => {
                *state.drive.session() = Some(logged);
                return Err(e);
            }
            Err(e) => return Err(e),
        };
        if cancel.load(Ordering::SeqCst) {
            return Err(CmdError::new("cancelled", "cancelado"));
        }
        let summary = import_encrypted(&app, &state, bytes, &key, target)?;

        let media = if include_media {
            Some(download_media(&app, &state, &ep, token, &files, &key, target)?)
        } else {
            None
        };
        drop(logged); // tokens zerados aqui
        Ok(DriveResult { summary, media })
    })
    .await
}

/// Baixa as mídias que o histórico usa e ainda não estão no cofre.
///
/// * Backup sem E2E: arquivos `Media/...` em claro no Drive (MD5 conferido).
/// * Backup E2E: `<hash>.mcrypt1`, com o nome real nos metadados cifrados.
///   Se a tag GCM não conferir, o conteúdo só entra se o SHA-256 bater com o
///   `file_hash` que o msgstore guarda daquela mídia.
fn download_media(
    app: &AppHandle,
    state: &AppState,
    ep: &Endpoints,
    token: &wa_gdrive::AccessToken,
    files: &[BackupFile],
    key: &wa_crypt::BackupKey,
    target: SourceApp,
) -> Result<MediaSummary, CmdError> {
    let plan = Plan::load(state, target)?;
    let handle = state.vault().as_ref().ok_or_else(CmdError::locked)?.media(target)?;
    let writer = handle.writer()?;
    writer.cleanup();
    let mut summary = MediaSummary { expected: plan.len() as u64, ..Default::default() };
    let mut tally = crate::diag::Tally::new("mídias do Drive");
    tracing::info!(esperadas = plan.len(), arquivos_no_backup = files.len(), "mídias do Drive: planejando");

    // Decide o que baixar (e o nome real de cada .mcrypt1).
    let mut todo = Vec::new();
    let mut logged_meta = false;
    for f in files {
        let (name, hex) = match f.path.strip_suffix(".mcrypt1") {
            Some(stem) => {
                let hex = stem.rsplit('/').next().unwrap_or(stem).to_owned();
                match mcrypt1_name(key, f, &mut logged_meta) {
                    Ok(n) => (n, Some(hex)),
                    Err(reason) => {
                        tally.add(reason.0, reason.1);
                        summary.unmatched += 1;
                        continue;
                    }
                }
            }
            None => (f.path.clone(), None),
        };
        match plan.get(&name) {
            Some(want) => todo.push((f, want, hex)),
            None if name.contains("Media/") => summary.unmatched += 1,
            None => {}
        }
    }
    summary.matched = todo.len() as u64;
    tracing::info!(
        casadas = summary.matched,
        sem_correspondencia = summary.unmatched,
        tamanho = %crate::diag::mb(todo.iter().map(|(f, _, _)| f.size_bytes).sum()),
        mcrypt1 = todo.iter().filter(|(_, _, h)| h.is_some()).count(),
        "mídias do Drive: a baixar"
    );
    let total_bytes = todo.iter().map(|(f, _, _)| f.size_bytes).sum();
    let mut progress = crate::media_import::Progress::new(app, todo.len() as u64, total_bytes);
    let api = Api::new(ep, token)?;
    let cancel = state.drive.cancel.clone();
    // (com tag GCM válida, sem tag mas conferida pelo file_hash)
    let mut mcrypt_stats = (0u64, 0u64);

    for (file, want, hex) in todo {
        if cancel.load(Ordering::SeqCst) || state.media_cancel.load(Ordering::SeqCst) {
            summary.stopped = Some("cancelado".into());
            break;
        }
        if already_stored(&writer, want) {
            summary.already += 1;
        } else {
            match api.download(file, &cancel, |_, _| {}) {
                Ok(bytes) => match hex {
                    None => store_one(&writer, &mut summary, &mut tally, want, &bytes[..], None),
                    Some(hex) => match wa_crypt::mcrypt1::decrypt_media(key, &hex, bytes) {
                        Ok(m) if m.verified => {
                            mcrypt_stats.0 += 1;
                            store_one(&writer, &mut summary, &mut tally, want, &m.data[..], None)
                        }
                        // Sem autenticação: só com o hash do WhatsApp para conferir.
                        Ok(m) => match want.file_hash.as_deref() {
                            Some(h) => {
                                mcrypt_stats.1 += 1;
                                store_one(&writer, &mut summary, &mut tally, want, &m.data[..], Some(h))
                            }
                            None => {
                                tally.add("mcrypt1 sem tag e sem file_hash", "não há como conferir o conteúdo");
                                summary.failed += 1
                            }
                        },
                        Err(e) => {
                            tally.add("mcrypt1 não decifrou", e);
                            summary.failed += 1
                        }
                    },
                },
                Err(wa_gdrive::Error::TokenExpired) => {
                    summary.stopped = Some("a sessão do Google expirou; entre de novo para continuar".into());
                    break;
                }
                Err(wa_gdrive::Error::Cancelled) => {
                    summary.stopped = Some("cancelado".into());
                    break;
                }
                Err(e) => {
                    tally.add("download falhou", e);
                    summary.failed += 1
                }
            }
        }
        summary.bytes += file.size_bytes;
        progress.report(&summary, false);
    }
    progress.report(&summary, true);
    tally.report();
    tracing::info!(
        adicionadas = summary.added,
        ja_no_cofre = summary.already,
        falharam = summary.failed,
        mcrypt1_com_tag = mcrypt_stats.0,
        mcrypt1_conferidas_por_hash = mcrypt_stats.1,
        interrompido = ?summary.stopped,
        "mídias do Drive: fim"
    );
    Ok(summary)
}

/// Nome real de um `.mcrypt1`, lido dos metadados cifrados. No erro, devolve
/// (categoria, detalhe) para o diagnóstico. Na primeira vez, registra as
/// chaves (não os valores) do JSON de metadados, para validar o formato.
fn mcrypt1_name(
    key: &wa_crypt::BackupKey,
    f: &BackupFile,
    logged: &mut bool,
) -> Result<String, (&'static str, String)> {
    use base64::Engine;
    use base64::engine::general_purpose::STANDARD as B64;

    let meta = f.metadata.as_deref().ok_or(("mcrypt1 sem metadados", String::from("campo metadata vazio")))?;
    let raw = B64.decode(meta).map_err(|e| ("metadados não são base64", e.to_string()))?;
    let json = wa_crypt::mcrypt1::decrypt_metadata(key, &raw).map_err(|e| ("metadados não decifraram", format!("{e} ({} bytes)", raw.len())))?;
    let v: serde_json::Value =
        serde_json::from_slice(&json).map_err(|e| ("metadados decifrados não são JSON", format!("{e} ({} bytes)", json.len())))?;
    if !*logged {
        *logged = true;
        let keys: Vec<&String> = v.as_object().map(|o| o.keys().collect()).unwrap_or_default();
        let name = v.get("name").and_then(|n| n.as_str());
        tracing::info!(
            chaves = ?keys,
            name_comeca_com_media = ?name.map(|n| n.starts_with("Media/")),
            "metadados do primeiro .mcrypt1"
        );
    }
    v.get("name")
        .and_then(|n| n.as_str())
        .map(str::to_owned)
        .ok_or(("metadados sem o campo name", String::from("JSON sem name")))
}

/// Resumo da listagem de arquivos do backup: contagens por pasta e extensão.
/// Só os arquivos de `Databases/` aparecem pelo nome.
fn log_files(files: &[BackupFile]) {
    use std::collections::BTreeMap;
    let mut by_dir: BTreeMap<String, (u64, u64)> = BTreeMap::new();
    let mut by_ext: BTreeMap<String, u64> = BTreeMap::new();
    let mut databases = Vec::new();
    for f in files {
        let top = f.path.split_once('/').map(|(t, _)| t).unwrap_or("(raiz)").to_owned();
        let e = by_dir.entry(top).or_default();
        e.0 += 1;
        e.1 += f.size_bytes;
        let ext = f.path.rsplit_once('.').map(|(_, x)| x).filter(|x| x.len() <= 8 && !x.contains('/')).unwrap_or("");
        *by_ext.entry(ext.to_lowercase()).or_default() += 1;
        if f.path.starts_with("Databases/") {
            databases.push(format!("{} ({})", f.path, crate::diag::mb(f.size_bytes)));
        }
    }
    let dirs: Vec<String> = by_dir.iter().map(|(d, (n, b))| format!("{d}: {n} arq, {}", crate::diag::mb(*b))).collect();
    tracing::info!(total = files.len(), pastas = ?dirs, extensoes = ?by_ext, "arquivos do backup no Drive");
    tracing::info!(bancos = ?databases, "arquivos em Databases/");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn navegacao_permitida_na_janela_de_login() {
        let ok = |u: &str| allowed_navigation(&u.parse().unwrap());
        assert!(ok("https://accounts.google.com/EmbeddedSetup"));
        assert!(ok("https://accounts.google.com.br/accounts/SetSID"));
        assert!(ok("https://www.google.co.uk/"));
        assert!(ok("https://ssl.gstatic.com/x.js"));
        assert!(ok("https://accounts.youtube.com/accounts/SetSID"));
        assert!(!ok("http://accounts.google.com/"));
        assert!(!ok("https://google.com.evil.example/"));
        assert!(!ok("https://notgoogle.com/"));
        assert!(!ok("https://google.evil-site.com/"));
        assert!(!ok("https://evil.com/?google.com"));
        assert!(!ok("https://google.us.com/"));
        assert!(!ok("https://x.google.evil.com/"));
        assert!(!ok("javascript:alert(1)"));
    }

    #[test]
    fn email() {
        assert!(check_email("a@b.com").is_ok());
        assert!(check_email("ab.com").is_err());
        assert!(check_email("@b.com").is_err());
    }
}
