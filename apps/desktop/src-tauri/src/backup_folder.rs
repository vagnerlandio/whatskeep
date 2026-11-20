//! The copy of a Google Drive backup saved to a folder the user chooses.
//!
//! The folder mirrors WhatsApp's own folder on the phone (`Databases/`,
//! `Backups/`, `Media/`), so it can be imported again later (or restored on a
//! phone) without Google. Files are written exactly as they come from Drive:
//! the databases stay encrypted (crypt14/15) and so do `.mcrypt1` media.
//! Media of backups that do not encrypt them are plain files, as on the
//! phone. The copy belongs to the user (it is their backup, like the folder
//! copied from the phone); the vault is still the only place the app keeps
//! decrypted data.
//!
//! Downloads are resumable: each file is written to `<name>.partial` and only
//! renamed after its size and MD5 match, so a file with its final name and the
//! announced size is complete and is skipped next time.

use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use wk_gdrive::{Api, Backup, BackupFile};
use wk_import::SourceApp;

use crate::error::CmdError;

/// Drive listing kept next to the files: sizes and the (still encrypted)
/// metadata of `.mcrypt1` media, which holds their real names. Hidden, so the
/// media import does not walk into it.
pub const MANIFEST: &str = ".whatskeep-drive.json";
const MANIFEST_FORMAT: u32 = 1;
const EVENT: &str = "gdrive-progress";
/// Parallel downloads. Each request has ~0.7 s of latency, so one at a time
/// spends most of the time waiting.
pub const WORKERS: usize = 8;
/// Extra attempts for a file after a network failure or a corrupted download.
const RETRIES: u32 = 2;

/// Stop reasons. The frontend matches these (Portuguese) strings exactly.
pub const STOPPED_CANCELLED: &str = "cancelado";
pub const STOPPED_EXPIRED: &str = "a sessão do Google expirou; entre de novo para continuar";

/// `whatsapp-backup-2026-11-19`: app and backup date, never the phone number.
pub fn folder_name(app: SourceApp, update_time: Option<&str>) -> String {
    let is_date = |d: &&str| {
        d.len() == 10 && d.bytes().enumerate().all(|(i, b)| if i == 4 || i == 7 { b == b'-' } else { b.is_ascii_digit() })
    };
    let date = update_time.and_then(|t| t.get(..10)).filter(is_date).map(str::to_owned).unwrap_or_else(|| {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        crate::export::date_label(now)
    });
    let prefix = match app {
        SourceApp::WhatsApp => "whatsapp",
        SourceApp::Business => "whatsapp-business",
    };
    format!("{prefix}-backup-{date}")
}

/// Where the backup goes inside the folder the user picked. Picking the backup
/// folder itself (to resume a download) uses it as is.
pub fn backup_root(picked: &Path, name: &str) -> PathBuf {
    if picked.file_name().is_some_and(|n| n == name) { picked.to_path_buf() } else { picked.join(name) }
}

/// A Drive path (`Media/WhatsApp Images/IMG.jpg`) as a relative path that
/// cannot leave the backup folder. `None` for anything unusual.
pub fn safe_relative(path: &str) -> Option<PathBuf> {
    if path.is_empty() || path.contains('\\') || path.chars().any(char::is_control) {
        return None;
    }
    // Segment by segment: `Path::components` silently drops `.` and empty parts.
    let mut out = PathBuf::new();
    for seg in path.split('/') {
        if seg.is_empty() || seg == "." || seg == ".." {
            return None;
        }
        match Path::new(seg).components().next() {
            Some(Component::Normal(_)) => out.push(seg),
            _ => return None,
        }
    }
    let name = out.file_name()?.to_str()?;
    // Our own names: never let Drive overwrite them.
    if name == MANIFEST || name.ends_with(".partial") {
        return None;
    }
    Some(out)
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub format: u32,
    pub app: String,
    pub update_time: Option<String>,
    pub encrypted: Option<bool>,
    pub app_version: Option<String>,
    pub files: Vec<ManifestFile>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestFile {
    pub path: String,
    pub size: u64,
    /// Encrypted metadata (base64) of `.mcrypt1` files.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<String>,
}

impl Manifest {
    pub fn new(app: SourceApp, backup: &Backup, files: &[BackupFile]) -> Self {
        Self {
            format: MANIFEST_FORMAT,
            app: app.as_str().to_owned(),
            update_time: backup.update_time.clone(),
            encrypted: backup.encrypted,
            app_version: backup.app_version.clone(),
            files: files
                .iter()
                .filter(|f| safe_relative(&f.path).is_some())
                .map(|f| ManifestFile { path: f.path.clone(), size: f.size_bytes, metadata: f.metadata.clone() })
                .collect(),
        }
    }

    /// Written atomically (temporary file + rename).
    pub fn save(&self, root: &Path) -> Result<(), CmdError> {
        let tmp = root.join(format!("{MANIFEST}.partial"));
        let json = serde_json::to_vec_pretty(self).map_err(|e| CmdError::new("internal", e.to_string()))?;
        std::fs::write(&tmp, json)?;
        std::fs::rename(&tmp, root.join(MANIFEST))?;
        Ok(())
    }

    /// The manifest of a backup folder, if it has one we understand.
    pub fn load(root: &Path) -> Option<Self> {
        let raw = std::fs::read(root.join(MANIFEST)).ok()?;
        let m: Self = serde_json::from_slice(&raw).ok()?;
        (m.format == MANIFEST_FORMAT).then_some(m)
    }

    pub fn metadata(&self, rel: &str) -> Option<&str> {
        self.files.iter().find(|f| f.path == rel).and_then(|f| f.metadata.as_deref())
    }
}

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadSummary {
    /// Files of the backup to download (the database included).
    pub files: u64,
    pub downloaded: u64,
    /// Already in the folder from an earlier run.
    pub already: u64,
    pub failed: u64,
    /// Bytes in the folder (downloaded now or before).
    pub bytes: u64,
    pub total_bytes: u64,
    pub stopped: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Progress {
    /// `database` (the msgstore, byte by byte) or `files` (everything else).
    phase: &'static str,
    files_done: u64,
    files_total: u64,
    bytes: u64,
    total_bytes: u64,
    failed: u64,
}

/// `bytes` of `total_bytes`: the database's own while it downloads, the
/// whole backup's afterwards.
fn emit(app: &AppHandle, phase: &'static str, s: &DownloadSummary, bytes: u64, total_bytes: u64) {
    let _ = app.emit(
        EVENT,
        Progress {
            phase,
            files_done: s.downloaded + s.already + s.failed,
            files_total: s.files,
            bytes,
            total_bytes,
            failed: s.failed,
        },
    );
}

/// A file with its final name and the announced size is complete.
fn complete(dest: &Path, size: u64) -> bool {
    std::fs::metadata(dest).is_ok_and(|m| m.is_file() && m.len() == size)
}

fn partial_path(dest: &Path) -> PathBuf {
    let mut name = dest.file_name().unwrap_or_default().to_os_string();
    name.push(".partial");
    dest.with_file_name(name)
}

/// Runs a download, retrying network failures and corrupted downloads.
pub fn with_retries<T>(
    cancel: &AtomicBool,
    mut attempt_once: impl FnMut() -> Result<T, wk_gdrive::Error>,
) -> Result<T, wk_gdrive::Error> {
    use wk_gdrive::Error as E;
    let mut attempt = 0;
    loop {
        match attempt_once() {
            Ok(v) => return Ok(v),
            Err(e) => {
                let transient = matches!(e, E::Network(_) | E::Incomplete | E::Checksum | E::Http(500..=599));
                if !transient || attempt >= RETRIES || cancel.load(Ordering::SeqCst) {
                    return Err(e);
                }
                attempt += 1;
                tracing::debug!(attempt, "retrying a download: {e}");
                std::thread::sleep(Duration::from_secs(attempt as u64));
            }
        }
    }
}

/// Progress of the message database download (also when it goes straight
/// to the vault, without a folder).
pub fn report_database(app: &AppHandle, done: u64, total: u64) {
    emit(app, "database", &DownloadSummary { files: 1, ..Default::default() }, done, total);
}

/// Downloads one file to `dest` (through `<dest>.partial`), retrying network
/// failures and corrupted downloads.
fn fetch(
    api: &Api,
    file: &BackupFile,
    dest: &Path,
    cancel: &AtomicBool,
    mut progress: impl FnMut(u64, u64),
) -> Result<u64, wk_gdrive::Error> {
    use wk_gdrive::Error as E;
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| E::Write(e.to_string()))?;
    }
    let tmp = partial_path(dest);
    with_retries(cancel, || {
        let result = (|| {
            let f = std::fs::File::create(&tmp).map_err(|e| E::Write(e.to_string()))?;
            let mut w = std::io::BufWriter::with_capacity(1 << 20, f);
            let n = api.download_into(file, &mut w, cancel, &mut progress)?;
            w.flush().map_err(|e| E::Write(e.to_string()))?;
            drop(w);
            std::fs::rename(&tmp, dest).map_err(|e| E::Write(e.to_string()))?;
            Ok(n)
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
        result
    })
}

/// Downloads the message database, reporting its progress byte by byte.
/// Returns the path where it is.
pub fn download_database(
    app: &AppHandle,
    api: &Api,
    root: &Path,
    db: &BackupFile,
    cancel: &AtomicBool,
    summary: &mut DownloadSummary,
) -> Result<PathBuf, CmdError> {
    let rel = safe_relative(&db.path).ok_or_else(|| CmdError::new("google", "nome de arquivo inesperado no backup"))?;
    let dest = root.join(rel);
    if complete(&dest, db.size_bytes) {
        tracing::info!(size = %crate::diag::mb(db.size_bytes), "message database already in the folder");
        summary.already += 1;
    } else {
        tracing::info!(file = db.path, size = %crate::diag::mb(db.size_bytes), md5 = db.md5.is_some(), "downloading the message database");
        let mut next_log = 0u64;
        let mut last = Instant::now() - Duration::from_secs(1);
        fetch(api, db, &dest, cancel, |done, total| {
            if total > 0 && done * 10 / total >= next_log {
                tracing::debug!(downloaded = %crate::diag::mb(done), total = %crate::diag::mb(total), "download {}%", done * 100 / total);
                next_log = done * 10 / total + 1;
            }
            if last.elapsed() >= Duration::from_millis(150) || done == total {
                last = Instant::now();
                emit(app, "database", summary, done, db.size_bytes);
            }
        })?;
        summary.downloaded += 1;
    }
    summary.bytes += db.size_bytes;
    emit(app, "database", summary, db.size_bytes, db.size_bytes);
    Ok(dest)
}

/// Downloads the other files in parallel. Per-file failures are counted and
/// the rest carries on; an expired session or a cancellation stops it all.
pub fn download_files(
    app: &AppHandle,
    api: &Api,
    root: &Path,
    files: &[&BackupFile],
    cancel: &AtomicBool,
    summary: &mut DownloadSummary,
) {
    let mut tally = crate::diag::Tally::new("Drive download");
    let mut todo = Vec::new();
    for f in files {
        match safe_relative(&f.path) {
            None => {
                tally.add("unexpected file name", "skipped");
                summary.failed += 1;
            }
            Some(rel) => {
                let dest = root.join(rel);
                if complete(&dest, f.size_bytes) {
                    summary.already += 1;
                    summary.bytes += f.size_bytes;
                } else {
                    todo.push((*f, dest));
                }
            }
        }
    }
    tracing::info!(
        to_download = todo.len(),
        already_in_folder = summary.already,
        size = %crate::diag::mb(todo.iter().map(|(f, _)| f.size_bytes).sum()),
        workers = WORKERS,
        "Drive download: files"
    );
    emit(app, "files", summary, summary.bytes, summary.total_bytes);

    let next = AtomicUsize::new(0);
    let downloaded = AtomicU64::new(0);
    let failed = AtomicU64::new(0);
    let bytes = AtomicU64::new(summary.bytes);
    let stop: Mutex<Option<&'static str>> = Mutex::new(None);
    let tally = Mutex::new(&mut tally);
    let last_emit = Mutex::new(Instant::now());
    let started = Instant::now();
    let base = summary.clone();

    std::thread::scope(|s| {
        for _ in 0..WORKERS.min(todo.len()) {
            s.spawn(|| {
                loop {
                    if cancel.load(Ordering::SeqCst) || stop.lock().unwrap_or_else(|e| e.into_inner()).is_some() {
                        return;
                    }
                    let i = next.fetch_add(1, Ordering::SeqCst);
                    let Some((file, dest)) = todo.get(i) else { return };
                    match fetch(api, file, dest, cancel, |_, _| {}) {
                        Ok(_) => {
                            downloaded.fetch_add(1, Ordering::SeqCst);
                            bytes.fetch_add(file.size_bytes, Ordering::SeqCst);
                        }
                        Err(wk_gdrive::Error::TokenExpired) => {
                            stop.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert(STOPPED_EXPIRED);
                            return;
                        }
                        Err(wk_gdrive::Error::Cancelled) => return,
                        Err(e) => {
                            failed.fetch_add(1, Ordering::SeqCst);
                            tally.lock().unwrap_or_else(|e| e.into_inner()).add("download failed", e);
                        }
                    }
                    let mut last = last_emit.lock().unwrap_or_else(|e| e.into_inner());
                    if last.elapsed() >= Duration::from_millis(250) {
                        *last = Instant::now();
                        let mut now = base.clone();
                        now.downloaded += downloaded.load(Ordering::SeqCst);
                        now.failed += failed.load(Ordering::SeqCst);
                        emit(app, "files", &now, bytes.load(Ordering::SeqCst), now.total_bytes);
                    }
                }
            });
        }
    });

    summary.downloaded += downloaded.into_inner();
    summary.failed += failed.into_inner();
    summary.bytes = bytes.into_inner();
    summary.stopped = stop.into_inner().unwrap_or_else(|e| e.into_inner()).map(str::to_owned);
    if summary.stopped.is_none() && cancel.load(Ordering::SeqCst) {
        summary.stopped = Some(STOPPED_CANCELLED.to_owned());
    }
    emit(app, "files", summary, summary.bytes, summary.total_bytes);
    tally.into_inner().unwrap_or_else(|e| e.into_inner()).report();
    let secs = started.elapsed().as_secs_f64().max(0.001);
    tracing::info!(
        downloaded = summary.downloaded,
        already_in_folder = summary.already,
        failed = summary.failed,
        stopped = ?summary.stopped,
        s = secs as u64,
        files_per_s = format!("{:.1}", summary.downloaded as f64 / secs),
        "Drive download: done"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_paths_stay_inside_the_folder() {
        assert_eq!(safe_relative("Media/WhatsApp Images/IMG-1.jpg"), Some(PathBuf::from("Media/WhatsApp Images/IMG-1.jpg")));
        assert_eq!(safe_relative("Databases/msgstore.db.crypt15"), Some(PathBuf::from("Databases/msgstore.db.crypt15")));
        assert_eq!(safe_relative("ab12.mcrypt1"), Some(PathBuf::from("ab12.mcrypt1")));
        for bad in [
            "",
            "/etc/passwd",
            "../x",
            "Media/../../x",
            "Media/./x",
            "Media\\..\\x",
            "Media/a\nb",
            "Media/x.jpg.partial",
            MANIFEST,
            "Media/",
        ] {
            assert_eq!(safe_relative(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn folder_names() {
        assert_eq!(folder_name(SourceApp::WhatsApp, Some("2026-11-19T21:59:23Z")), "whatsapp-backup-2026-11-19");
        assert_eq!(folder_name(SourceApp::Business, Some("2026-01-02")), "whatsapp-business-backup-2026-01-02");
        // An odd date falls back to today (still a valid name).
        let odd = folder_name(SourceApp::WhatsApp, Some("../../../x"));
        assert!(odd.starts_with("whatsapp-backup-") && !odd.contains('/'));
        let p = Path::new("/Volumes/SSD/whatsapp-backup-2026-11-19");
        assert_eq!(backup_root(p, "whatsapp-backup-2026-11-19"), p);
        assert_eq!(backup_root(Path::new("/Volumes/SSD"), "whatsapp-backup-2026-11-19"), p);
    }

    #[test]
    fn manifest_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let m = Manifest {
            format: MANIFEST_FORMAT,
            app: "whatsapp".into(),
            update_time: None,
            encrypted: Some(true),
            app_version: None,
            files: vec![
                ManifestFile { path: "Media/ab.mcrypt1".into(), size: 3, metadata: Some("AAAA".into()) },
                ManifestFile { path: "Media/x.jpg".into(), size: 4, metadata: None },
            ],
        };
        m.save(dir.path()).unwrap();
        let back = Manifest::load(dir.path()).unwrap();
        assert_eq!(back.metadata("Media/ab.mcrypt1"), Some("AAAA"));
        assert_eq!(back.metadata("Media/x.jpg"), None);
        assert!(!dir.path().join(format!("{MANIFEST}.partial")).exists());
    }

    #[test]
    fn partial_name() {
        assert_eq!(partial_path(Path::new("/b/Media/x.jpg")), PathBuf::from("/b/Media/x.jpg.partial"));
    }
}
