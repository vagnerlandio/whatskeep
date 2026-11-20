//! Backup download straight from Google Drive (unofficial flow).
//!
//! 1. A **private** window (no persistent cookies) opens Google's
//!    `EmbeddedSetup` login. It has no IPC permission at all (capabilities
//!    only apply to the `main` window) and only navigates Google domains.
//! 2. The backend reads the `oauth_token` cookie as soon as the login
//!    finishes, and the window is destroyed (along with its browser session).
//! 3. The cookie becomes the master token and, right away, `drive.appdata`
//!    access tokens (1 h) for WhatsApp and Business. The master token is
//!    zeroed right after.
//! 4. The access tokens stay in memory only until the download finishes, the
//!    user cancels, the vault locks or 50 minutes pass.
//!
//! The downloaded backup is still encrypted (crypt14/15) and goes straight
//! from memory to decryption and import: nothing is written to disk.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, Url, WebviewUrl, WebviewWindowBuilder};
use wk_gdrive::{Api, Backup, BackupFile, DriveApp, Endpoints, OauthCookie, Session};
use wk_import::SourceApp;
use zeroize::Zeroizing;

use crate::commands::{ImportSummary, KeyInput, import_encrypted, progress, source_app};
use crate::error::CmdError;
use crate::media_import::{MediaSummary, Plan, already_stored, store_one};
use crate::state::AppState;

const LOGIN_WINDOW: &str = "google-login";
const LOGIN_USER_AGENT: &str =
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/26.0 Safari/605.1.15";
const LOGIN_TIMEOUT: Duration = Duration::from_secs(15 * 60);
const SESSION_TTL: Duration = Duration::from_secs(50 * 60);
const EVENT_DOWNLOAD: &str = "gdrive-progress";

/// Domains the Google login goes through. Any other navigation in the login
/// window is blocked.
const ALLOWED_DOMAINS: &[&str] = &["gstatic.com", "googleusercontent.com", "googleapis.com", "youtube.com"];

#[derive(Default)]
pub struct DriveState {
    session: Mutex<Option<Logged>>,
    /// Cancellation request for the login or download in progress.
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
    /// Apps whose token can see this backup.
    apps: Vec<DriveApp>,
}

impl DriveState {
    fn session(&self) -> MutexGuard<'_, Option<Logged>> {
        self.session.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Discards the session (the tokens are zeroed on drop).
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

/// Cancels whatever is in progress, discards the tokens and closes the login window.
pub fn shutdown(app: &AppHandle) {
    let state = app.state::<AppState>();
    state.drive.cancel.store(true, Ordering::SeqCst);
    state.drive.clear();
    if let Some(w) = app.get_webview_window(LOGIN_WINDOW) {
        let _ = w.destroy();
    }
}

fn allowed_navigation(url: &Url) -> bool {
    // Empty frames the Google page creates via script. They do not touch the network.
    if url.scheme() == "about" {
        return matches!(url.path(), "blank" | "srcdoc");
    }
    if url.scheme() != "https" {
        return false;
    }
    let Some(host) = url.host_str() else { return false };
    if ALLOWED_DOMAINS.iter().any(|d| host == *d || host.ends_with(&format!(".{d}"))) {
        return true;
    }
    // google.com, google.de, accounts.google.com.br, google.co.uk. Only these
    // suffix shapes: something like `google.us.com` can be registered by anyone.
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

impl From<wk_gdrive::Error> for CmdError {
    fn from(e: wk_gdrive::Error) -> Self {
        use wk_gdrive::Error as E;
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
    /// Index in the session. The real backup name (`clients/wa/backups/<phone>`)
    /// never comes back from the webview: the download uses this index.
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
    /// Apps Google refused (e.g. an account without WhatsApp Business).
    refused: Vec<DriveApp>,
}

/// Opens the Google login window and waits for the user to sign in.
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

    let mut login_url: Url = wk_gdrive::LOGIN_URL.parse().expect("valid fixed URL");
    // Google's page prefills the e-mail with this: the user does not type it twice.
    login_url.query_pairs_mut().append_pair("Email", email.trim());
    WebviewWindowBuilder::new(&app, LOGIN_WINDOW, WebviewUrl::External(login_url))
        .title(crate::settings::tr(&app, "Entrar no Google", "Sign in to Google"))
        .inner_size(480.0, 720.0)
        .incognito(true)
        // WKWebView identifies itself without the Safari marker, and Google refuses
        // embedded browsers ("this browser or app may not be secure").
        .user_agent(LOGIN_USER_AGENT)
        .on_navigation(|url| {
            let ok = allowed_navigation(url);
            // Host and path only (no query): no e-mail or codes.
            let shown = format!("{}{}", url.host_str().unwrap_or(""), wk_gdrive::trace::scrub_digits(url.path()));
            if ok {
                tracing::debug!("Google login: navigated to {shown}");
            } else {
                tracing::warn!(scheme = url.scheme(), "Google login: navigation BLOCKED to {shown}");
            }
            ok
        })
        .build()
        .map_err(|e| CmdError::new("internal", format!("não foi possível abrir a janela de login: {e}")))?;

    let app2 = app.clone();
    let cookie = crate::commands::blocking("gdrive_login_waiting_for_cookie", move || {
        // The user interacts with the Google window, not the main one: without
        // this, the autolock would lock the vault (and cancel the login) midway.
        // The login itself has a time limit (LOGIN_TIMEOUT).
        let state = app2.state::<AppState>();
        let _busy = state.busy();
        wait_for_cookie(&app2)
    })
    .await?;
    finish_login(app, email, cookie).await
}

/// Fallback: the user signed in in the browser and pasted the `oauth_token` cookie.
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

/// Reads the window's cookies until `oauth_token` shows up (or the window closes).
fn wait_for_cookie(app: &AppHandle) -> Result<OauthCookie, CmdError> {
    let accounts: Url = "https://accounts.google.com/".parse().expect("valid fixed URL");
    let cancel = app.state::<AppState>().drive.cancel.clone();
    let started = Instant::now();
    let mut last_names: Vec<String> = Vec::new();
    tracing::info!("Google login: window open, waiting for the oauth_token cookie");
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
                tracing::debug!("Google login: could not read cookies: {e}");
                continue;
            }
        };
        // Only the cookie names (never the values), when they change.
        let mut names: Vec<String> = cookies.iter().map(|c| c.name().to_owned()).collect();
        names.sort();
        if names != last_names {
            tracing::debug!(cookies = ?names, "Google login: accounts.google.com cookies");
            last_names = names;
        }
        if let Some(c) = cookies.iter().find(|c| c.name() == wk_gdrive::LOGIN_COOKIE && !c.value().is_empty()) {
            let cookie = OauthCookie::new(c.value().to_owned());
            drop(cookies);
            tracing::info!(s = started.elapsed().as_secs(), "Google login: oauth_token cookie captured");
            // Closes the window: the private browser session goes away with it.
            let _ = window.destroy();
            return Ok(cookie);
        }
    }
}

async fn finish_login(app: AppHandle, email: String, cookie: OauthCookie) -> Result<LoginResult, CmdError> {
    crate::commands::blocking("finish_login", move || {
        let ep = Endpoints::default();
        let session = wk_gdrive::login(&ep, &email, cookie)?;

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
            tracing::warn!("listing backups failed: {e}");
        }
        tracing::info!(
            backups = found.len(),
            apps_with_token = ?session.tokens.iter().map(|(a, _)| a.package()).collect::<Vec<_>>(),
            apps_refused = ?session.refused.iter().map(|(a, e)| format!("{}: {e}", a.package())).collect::<Vec<_>>(),
            "Google login done"
        );
        for (i, f) in found.iter().enumerate() {
            let b = &f.backup;
            tracing::info!(
                id = i,
                apps = ?f.apps.iter().map(|a| a.package()).collect::<Vec<_>>(),
                updated = ?b.update_time,
                size = ?b.size_bytes.map(crate::diag::mb),
                database = ?b.chat_db_size.map(crate::diag::mb),
                media = ?b.media_size.map(crate::diag::mb),
                messages = ?b.messages,
                e2e = ?b.encrypted,
                uploading = b.uploading,
                app_version = ?b.app_version,
                "backup found"
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
        // The vault may have locked during the login: then the tokens are not kept at all.
        require_unlocked(&state)?;
        *state.drive.session() = Some(Logged { session, backups: found, at: Instant::now() });
        Ok(LoginResult { backups: dto, refused })
    })
    .await
}

/// Discards the Google tokens and closes the login window.
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

/// Downloads the chosen backup's msgstore, decrypts and imports it; then, if
/// requested, the media. When done (successfully or not), the Google tokens
/// are discarded.
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
        // A wrong or malformed key fails before anything is downloaded.
        let key = key.load()?;
        require_unlocked(&state)?;

        // The session leaves the state: the tokens die at the end of this command.
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
            let db = wk_gdrive::find_msgstore(&files).ok_or_else(|| {
                CmdError::new("not_msgstore", "este backup não tem o banco de mensagens (msgstore) no Google Drive")
            })?;
            tracing::info!(file = db.path, size = %crate::diag::mb(db.size_bytes), md5 = db.md5.is_some(), "downloading the message database");
            let mut next_log = 0u64;
            let bytes = api.download(db, &cancel, |downloaded, total| {
                if total > 0 && downloaded * 10 / total >= next_log {
                    tracing::debug!(downloaded = %crate::diag::mb(downloaded), total = %crate::diag::mb(total), "download {}%", downloaded * 100 / total);
                    next_log = downloaded * 10 / total + 1;
                }
                let _ = app.emit(EVENT_DOWNLOAD, DownloadProgress { downloaded, total });
            })?;
            Ok((bytes, files))
        })();
        let (bytes, files) = match downloaded {
            Ok(v) => v,
            // Network failure or corrupted file: the session is put back so the
            // user can retry without logging in again (it still expires at SESSION_TTL).
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
        drop(logged); // tokens zeroed here
        Ok(DriveResult { summary, media })
    })
    .await
}

/// Downloads the media the history uses that are not in the vault yet.
///
/// * Backup without E2E: plaintext `Media/...` files on Drive (MD5 checked).
/// * E2E backup: `<hash>.mcrypt1`, with the real name in the encrypted metadata.
///   If the GCM tag does not verify, the content only gets in if its SHA-256
///   matches the `file_hash` the msgstore keeps for that media.
fn download_media(
    app: &AppHandle,
    state: &AppState,
    ep: &Endpoints,
    token: &wk_gdrive::AccessToken,
    files: &[BackupFile],
    key: &wk_crypt::BackupKey,
    target: SourceApp,
) -> Result<MediaSummary, CmdError> {
    let plan = Plan::load(state, target)?;
    let handle = state.vault().as_ref().ok_or_else(CmdError::locked)?.media(target)?;
    let writer = handle.writer()?;
    writer.cleanup();
    let mut summary = MediaSummary { expected: plan.len() as u64, ..Default::default() };
    let mut tally = crate::diag::Tally::new("Drive media");
    tracing::info!(expected = plan.len(), files_in_backup = files.len(), "Drive media: planning");

    // Decides what to download (and the real name of each .mcrypt1).
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
        matched = summary.matched,
        unmatched = summary.unmatched,
        size = %crate::diag::mb(todo.iter().map(|(f, _, _)| f.size_bytes).sum()),
        mcrypt1 = todo.iter().filter(|(_, _, h)| h.is_some()).count(),
        "Drive media: to download"
    );
    let total_bytes = todo.iter().map(|(f, _, _)| f.size_bytes).sum();
    let mut progress = crate::media_import::Progress::new(app, todo.len() as u64, total_bytes);
    let api = Api::new(ep, token)?;
    let cancel = state.drive.cancel.clone();
    // (with a valid GCM tag, without a tag but checked against file_hash)
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
                    Some(hex) => match wk_crypt::mcrypt1::decrypt_media(key, &hex, bytes) {
                        Ok(m) if m.verified => {
                            mcrypt_stats.0 += 1;
                            store_one(&writer, &mut summary, &mut tally, want, &m.data[..], None)
                        }
                        // Not authenticated: only with WhatsApp's hash to check against.
                        Ok(m) => match want.file_hash.as_deref() {
                            Some(h) => {
                                mcrypt_stats.1 += 1;
                                store_one(&writer, &mut summary, &mut tally, want, &m.data[..], Some(h))
                            }
                            None => {
                                tally.add("mcrypt1 without tag or file_hash", "no way to verify the content");
                                summary.failed += 1
                            }
                        },
                        Err(e) => {
                            tally.add("mcrypt1 did not decrypt", e);
                            summary.failed += 1
                        }
                    },
                },
                Err(wk_gdrive::Error::TokenExpired) => {
                    summary.stopped = Some("a sessão do Google expirou; entre de novo para continuar".into());
                    break;
                }
                Err(wk_gdrive::Error::Cancelled) => {
                    summary.stopped = Some("cancelado".into());
                    break;
                }
                Err(e) => {
                    tally.add("download failed", e);
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
        added = summary.added,
        already_in_vault = summary.already,
        failed = summary.failed,
        mcrypt1_with_tag = mcrypt_stats.0,
        mcrypt1_checked_by_hash = mcrypt_stats.1,
        stopped = ?summary.stopped,
        "Drive media: done"
    );
    Ok(summary)
}

/// Real name of a `.mcrypt1`, read from the encrypted metadata. On error,
/// returns (category, detail) for diagnostics. The first time, it logs the
/// keys (not the values) of the metadata JSON, to validate the format.
fn mcrypt1_name(
    key: &wk_crypt::BackupKey,
    f: &BackupFile,
    logged: &mut bool,
) -> Result<String, (&'static str, String)> {
    use base64::Engine;
    use base64::engine::general_purpose::STANDARD as B64;

    let meta = f.metadata.as_deref().ok_or(("mcrypt1 without metadata", String::from("empty metadata field")))?;
    let raw = B64.decode(meta).map_err(|e| ("metadata is not base64", e.to_string()))?;
    let json = wk_crypt::mcrypt1::decrypt_metadata(key, &raw).map_err(|e| ("metadata did not decrypt", format!("{e} ({} bytes)", raw.len())))?;
    let v: serde_json::Value =
        serde_json::from_slice(&json).map_err(|e| ("decrypted metadata is not JSON", format!("{e} ({} bytes)", json.len())))?;
    if !*logged {
        *logged = true;
        let keys: Vec<&String> = v.as_object().map(|o| o.keys().collect()).unwrap_or_default();
        let name = v.get("name").and_then(|n| n.as_str());
        tracing::info!(
            keys = ?keys,
            name_starts_with_media = ?name.map(|n| n.starts_with("Media/")),
            "metadata of the first .mcrypt1"
        );
    }
    v.get("name")
        .and_then(|n| n.as_str())
        .map(str::to_owned)
        .ok_or(("metadata without the name field", String::from("JSON without name")))
}

/// Summary of the backup's file listing: counts by folder and extension.
/// Only the `Databases/` files appear by name.
fn log_files(files: &[BackupFile]) {
    use std::collections::BTreeMap;
    let mut by_dir: BTreeMap<String, (u64, u64)> = BTreeMap::new();
    let mut by_ext: BTreeMap<String, u64> = BTreeMap::new();
    let mut databases = Vec::new();
    for f in files {
        let top = f.path.split_once('/').map(|(t, _)| t).unwrap_or("(root)").to_owned();
        let e = by_dir.entry(top).or_default();
        e.0 += 1;
        e.1 += f.size_bytes;
        let ext = f.path.rsplit_once('.').map(|(_, x)| x).filter(|x| x.len() <= 8 && !x.contains('/')).unwrap_or("");
        *by_ext.entry(ext.to_lowercase()).or_default() += 1;
        if f.path.starts_with("Databases/") {
            databases.push(format!("{} ({})", f.path, crate::diag::mb(f.size_bytes)));
        }
    }
    let dirs: Vec<String> = by_dir.iter().map(|(d, (n, b))| format!("{d}: {n} files, {}", crate::diag::mb(*b))).collect();
    tracing::info!(total = files.len(), folders = ?dirs, extensions = ?by_ext, "backup files on Drive");
    tracing::info!(databases = ?databases, "files in Databases/");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allowed_navigation_in_login_window() {
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
        assert!(ok("about:blank"));
        assert!(ok("about:srcdoc"));
        assert!(!ok("about:config"));
        assert!(!ok("data:text/html,<script>"));
        assert!(!ok("file:///etc/passwd"));
    }

    #[test]
    fn email() {
        assert!(check_email("a@b.com").is_ok());
        assert!(check_email("ab.com").is_err());
        assert!(check_email("@b.com").is_err());
    }
}
