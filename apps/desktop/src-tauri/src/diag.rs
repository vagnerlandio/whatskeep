//! Diagnostics log, for following tests with real backups.
//!
//! * Always on in development builds and, in release, only with the
//!   `WA_LOG` variable (e.g. `WA_LOG=debug`). Goes to stderr: the app writes
//!   nothing to disk.
//! * **Never** log message text, phone numbers, JIDs, contact names,
//!   passwords, keys, tokens, cookies, URLs with a backup path or media file
//!   names. Only phases, timings, sizes, counts, error codes and WhatsApp
//!   table/column names.

use std::collections::BTreeMap;
use std::time::Instant;

use tracing::{info, warn};

/// Default filter: the app and its crates at `debug`, everything else at `info`.
const DEFAULT_FILTER: &str = "info,wa_historico_lib=debug,wa_import=debug,wa_gdrive=debug";

pub fn init() {
    let configured = std::env::var("WA_LOG").ok();
    if configured.is_none() && !cfg!(debug_assertions) {
        return;
    }
    let filter = tracing_subscriber::EnvFilter::new(configured.as_deref().unwrap_or(DEFAULT_FILTER));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .with_target(true)
        .try_init();

    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |p| {
        // Panic location and message: panic messages in the app's code do not
        // carry user data.
        tracing::error!(target: "wa_historico_lib::panic", "PANIC {p}");
        default_hook(p);
    }));
    info!(version = env!("CARGO_PKG_VERSION"), debug = cfg!(debug_assertions), "diagnostics enabled");
}

/// Times a step: `let _t = diag::timer("decrypt");` logs the duration when
/// it goes out of scope.
pub struct Timer {
    what: &'static str,
    start: Instant,
}

pub fn timer(what: &'static str) -> Timer {
    Timer { what, start: Instant::now() }
}

impl Timer {
    pub fn ms(&self) -> u128 {
        self.start.elapsed().as_millis()
    }
}

impl Drop for Timer {
    fn drop(&mut self) {
        tracing::debug!(step = self.what, ms = self.ms(), "step done");
    }
}

/// Counts failures by reason. The first ones of each reason are logged right
/// away (with detail); at the end, `report` sums everything up in one line.
pub struct Tally {
    what: &'static str,
    counts: BTreeMap<String, u64>,
}

const DETAILED_PER_REASON: u64 = 3;

impl Tally {
    pub fn new(what: &'static str) -> Self {
        Self { what, counts: BTreeMap::new() }
    }

    /// `reason` is a short, stable category; `detail` is the error message
    /// (already free of user data).
    pub fn add(&mut self, reason: &str, detail: impl std::fmt::Display) {
        let n = self.counts.entry(reason.to_owned()).or_default();
        *n += 1;
        if *n <= DETAILED_PER_REASON {
            warn!(context = self.what, reason = reason, "failure: {detail}");
        }
    }

    pub fn report(&self) {
        if !self.counts.is_empty() {
            warn!(context = self.what, failures = ?self.counts, "failure summary");
        }
    }
}

/// Frontend errors (JS exceptions, rendering failures). The text is
/// truncated and must not contain message data.
#[tauri::command]
pub fn ui_log(level: String, message: String) {
    let msg: String = message.chars().take(1000).collect();
    match level.as_str() {
        "error" => tracing::error!(target: "wa_historico_lib::ui", "{msg}"),
        "warn" => tracing::warn!(target: "wa_historico_lib::ui", "{msg}"),
        _ => tracing::info!(target: "wa_historico_lib::ui", "{msg}"),
    }
}

/// Human-readable size, for the log.
pub fn mb(bytes: u64) -> String {
    format!("{:.1} MB", bytes as f64 / 1_048_576.0)
}
