//! Log de diagnóstico, para acompanhar testes com backups reais.
//!
//! * Ligado sempre no build de desenvolvimento e, no release, só com a
//!   variável `WA_LOG` (ex.: `WA_LOG=debug`). Vai para o stderr: nada é
//!   gravado em disco pelo app.
//! * **Nunca** registre texto de mensagem, telefone, JID, nome de contato,
//!   senha, chave, token, cookie, URL com caminho de backup ou nome de
//!   arquivo de mídia. Só fases, tempos, tamanhos, contagens, códigos de erro
//!   e nomes de tabelas/colunas do WhatsApp.

use std::collections::BTreeMap;
use std::time::Instant;

use tracing::{info, warn};

/// Filtro padrão: o app e os crates em `debug`, o resto em `info`.
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
        // Local e mensagem do panic: mensagens de panic do código do app não
        // carregam dados do usuário.
        tracing::error!(target: "wa_historico_lib::panic", "PANIC {p}");
        default_hook(p);
    }));
    info!(version = env!("CARGO_PKG_VERSION"), debug = cfg!(debug_assertions), "diagnóstico ligado");
}

/// Mede uma etapa: `let _t = diag::timer("descriptografar");` registra a
/// duração quando sai de escopo.
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
        tracing::debug!(etapa = self.what, ms = self.ms(), "etapa concluída");
    }
}

/// Conta falhas por motivo. As primeiras de cada motivo saem no log na hora
/// (com detalhe); no fim, `report` resume tudo numa linha.
pub struct Tally {
    what: &'static str,
    counts: BTreeMap<String, u64>,
}

const DETAILED_PER_REASON: u64 = 3;

impl Tally {
    pub fn new(what: &'static str) -> Self {
        Self { what, counts: BTreeMap::new() }
    }

    /// `reason` é uma categoria curta e estável; `detail`, a mensagem de erro
    /// (já sem dados do usuário).
    pub fn add(&mut self, reason: &str, detail: impl std::fmt::Display) {
        let n = self.counts.entry(reason.to_owned()).or_default();
        *n += 1;
        if *n <= DETAILED_PER_REASON {
            warn!(contexto = self.what, motivo = reason, "falha: {detail}");
        }
    }

    pub fn report(&self) {
        if !self.counts.is_empty() {
            warn!(contexto = self.what, falhas = ?self.counts, "resumo das falhas");
        }
    }
}

/// Erros do frontend (exceções JS, falhas de renderização). O texto é
/// truncado e não deve conter dados de mensagens.
#[tauri::command]
pub fn ui_log(level: String, message: String) {
    let msg: String = message.chars().take(1000).collect();
    match level.as_str() {
        "error" => tracing::error!(target: "wa_historico_lib::ui", "{msg}"),
        "warn" => tracing::warn!(target: "wa_historico_lib::ui", "{msg}"),
        _ => tracing::info!(target: "wa_historico_lib::ui", "{msg}"),
    }
}

/// Tamanho legível, para o log.
pub fn mb(bytes: u64) -> String {
    format!("{:.1} MB", bytes as f64 / 1_048_576.0)
}
