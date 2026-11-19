//! Importação de mídias para o cofre: da pasta copiada do celular ou do
//! Google Drive.
//!
//! Só entram arquivos que alguma mensagem do histórico usa (pelo caminho no
//! celular). Mensagens apagadas não contam. Fotos de status, papéis de parede
//! e outros arquivos da pasta ficam de fora.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use wa_import::SourceApp;
use wa_vault::media::normalize_path;
use wa_vault::query::{self, MediaRef};
use wa_vault::{MediaWriter, Put};

use crate::commands::{blocking, source_app};
use crate::error::CmdError;
use crate::state::AppState;

const EVENT_MEDIA: &str = "media-progress";

/// O que o histórico espera, indexado pelo caminho normalizado (`Media/...`).
pub(crate) struct Plan {
    wanted: HashMap<String, MediaRef>,
}

impl Plan {
    pub fn load(state: &AppState, app: SourceApp) -> Result<Self, CmdError> {
        let refs = state.with_store(app, |c| Ok(query::media_refs(c)?))?;
        let wanted = refs
            .into_iter()
            .filter_map(|r| Some((normalize_path(r.file_path.as_deref()?)?, r)))
            .collect();
        Ok(Self { wanted })
    }

    pub fn get(&self, path: &str) -> Option<&MediaRef> {
        self.wanted.get(&normalize_path(path)?)
    }

    pub fn len(&self) -> usize {
        self.wanted.len()
    }
}

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaSummary {
    /// Arquivos do histórico encontrados na origem.
    pub matched: u64,
    pub added: u64,
    /// Já estavam no cofre.
    pub already: u64,
    /// Na origem, mas sem mensagem que os use.
    pub unmatched: u64,
    /// Falharam (leitura, integridade). Os demais continuam.
    pub failed: u64,
    pub bytes: u64,
    /// Mídias do histórico (com caminho conhecido).
    pub expected: u64,
    /// Por que parou antes do fim, se parou.
    pub stopped: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct MediaProgress {
    done: u64,
    total: u64,
    bytes: u64,
    total_bytes: u64,
    added: u64,
}

/// Emite progresso no máximo a cada 150 ms.
pub(crate) struct Progress<'a> {
    app: &'a AppHandle,
    last: Instant,
    pub total: u64,
    pub total_bytes: u64,
}

impl<'a> Progress<'a> {
    pub fn new(app: &'a AppHandle, total: u64, total_bytes: u64) -> Self {
        Self { app, last: Instant::now() - Duration::from_secs(1), total, total_bytes }
    }

    pub fn report(&mut self, s: &MediaSummary, force: bool) {
        if !force && self.last.elapsed() < Duration::from_millis(150) {
            return;
        }
        self.last = Instant::now();
        let _ = self.app.emit(
            EVENT_MEDIA,
            MediaProgress {
                done: s.added + s.already + s.failed,
                total: self.total,
                bytes: s.bytes,
                total_bytes: self.total_bytes,
                added: s.added,
            },
        );
    }
}

/// Grava um conteúdo no cofre, contando o resultado no resumo.
pub(crate) fn store_one(
    writer: &MediaWriter,
    summary: &mut MediaSummary,
    want: &MediaRef,
    data: impl std::io::Read,
    expected: Option<&str>,
) {
    match writer.put(data, expected, want.mime.as_deref(), want.file_path.as_deref()) {
        Ok((_, Put::Added)) => summary.added += 1,
        Ok((_, Put::AlreadyStored)) => summary.already += 1,
        Err(_) => summary.failed += 1,
    }
}

pub(crate) fn already_stored(writer: &MediaWriter, want: &MediaRef) -> bool {
    writer.find(want.file_hash.as_deref(), want.file_path.as_deref()).ok().flatten().is_some()
}

// ---------------------------------------------------------------- pasta local

/// Arquivos regulares sob `root`, sem seguir links simbólicos e sem entrar em
/// pastas ocultas (`.Statuses`, `.Shared`...).
fn walk(root: &Path, out: &mut Vec<PathBuf>, cancel: &dyn Fn() -> bool) {
    let Ok(entries) = std::fs::read_dir(root) else { return };
    for e in entries.flatten() {
        if cancel() {
            return;
        }
        if e.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        let Ok(ft) = e.file_type() else { continue };
        if ft.is_dir() {
            walk(&e.path(), out, cancel);
        } else if ft.is_file() {
            out.push(e.path());
        }
    }
}

/// Pede a pasta do WhatsApp copiada do celular e importa as mídias dela.
#[tauri::command]
pub async fn import_media_folder(app: AppHandle, source_app: String) -> Result<Option<MediaSummary>, CmdError> {
    use tauri_plugin_dialog::DialogExt;

    let target = self::source_app(&source_app)?;
    blocking(move || {
        // O seletor nativo roda no backend: o webview não escolhe caminhos.
        let Some(folder) = app
            .dialog()
            .file()
            .set_title("Escolha a pasta WhatsApp copiada do celular")
            .blocking_pick_folder()
        else {
            return Ok(None);
        };
        let root = folder.into_path().map_err(|_| CmdError::new("invalid_argument", "pasta inválida"))?;
        import_folder(&app, target, &root).map(Some)
    })
    .await
}

fn import_folder(app: &AppHandle, target: SourceApp, root: &Path) -> Result<MediaSummary, CmdError> {
    let state = app.state::<AppState>();
    let _busy = state.busy();
    state.media_cancel.store(false, Ordering::SeqCst);
    let plan = Plan::load(&state, target)?;
    let handle = state.vault().as_ref().ok_or_else(CmdError::locked)?.media(target)?;
    let writer = handle.writer()?;
    writer.cleanup();
    let cancelled = || state.media_cancel.load(Ordering::SeqCst);

    let mut files = Vec::new();
    walk(root, &mut files, &cancelled);
    let mut summary = MediaSummary { expected: plan.len() as u64, ..Default::default() };
    let mut todo = Vec::new();
    for path in files {
        let rel = path.to_string_lossy();
        match plan.get(&rel) {
            Some(want) => todo.push((path.clone(), want)),
            None => summary.unmatched += 1,
        }
    }
    summary.matched = todo.len() as u64;
    let total_bytes = todo.iter().filter_map(|(p, _)| p.metadata().ok()).map(|m| m.len()).sum();
    let mut progress = Progress::new(app, todo.len() as u64, total_bytes);

    for (path, want) in todo {
        if cancelled() {
            summary.stopped = Some("cancelado".into());
            break;
        }
        let size = path.metadata().map(|m| m.len()).unwrap_or(0);
        if already_stored(&writer, want) {
            summary.already += 1;
        } else {
            match std::fs::File::open(&path) {
                // Arquivo local: o conteúdo é o que o usuário tem. Não exige o
                // hash do WhatsApp (o arquivo pode ter sido recomprimido) e
                // fica achável pelo caminho.
                Ok(f) => store_one(&writer, &mut summary, want, std::io::BufReader::new(f), None),
                Err(_) => summary.failed += 1,
            }
        }
        summary.bytes += size;
        progress.report(&summary, false);
    }
    progress.report(&summary, true);
    Ok(summary)
}

/// Interrompe a importação de mídias em andamento (o que já entrou fica).
#[tauri::command]
pub fn media_cancel(app: AppHandle) {
    app.state::<AppState>().media_cancel.store(true, Ordering::SeqCst);
}

// ---------------------------------------------------------------- exportar

/// Salva uma mídia decifrada onde o usuário escolher (ação explícita: o
/// arquivo sai do cofre em claro).
#[tauri::command]
pub async fn save_media(app: AppHandle, store: String, message_id: i64) -> Result<bool, CmdError> {
    use tauri_plugin_dialog::DialogExt;

    let target = self::source_app(&store)?;
    blocking(move || {
        let state = app.state::<AppState>();
        let r = state
            .with_store(target, |c| Ok(query::media_ref(c, message_id)?))?
            .ok_or_else(|| CmdError::new("not_found", "mídia não encontrada"))?;
        let blob = state
            .with_media(target, |m| Ok(m.and_then(|m| m.find(r.file_hash.as_deref(), r.file_path.as_deref()).ok().flatten())))?
            .ok_or_else(|| CmdError::new("not_found", "o arquivo desta mídia não está no cofre"))?;
        let suggested = suggested_name(&r);
        let Some(dest) = app.dialog().file().set_file_name(&suggested).blocking_save_file() else {
            return Ok(false);
        };
        let dest = dest.into_path().map_err(|_| CmdError::new("invalid_argument", "destino inválido"))?;
        let tmp = dest.with_extension("parcial");
        let result = state.with_media(target, |m| {
            let m = m.ok_or_else(|| CmdError::new("not_found", "mídia não encontrada"))?;
            let f = std::fs::File::create(&tmp)?;
            m.copy_to(&blob, std::io::BufWriter::new(f))?;
            Ok(())
        });
        match result {
            Ok(()) => {
                std::fs::rename(&tmp, &dest)?;
                Ok(true)
            }
            Err(e) => {
                let _ = std::fs::remove_file(&tmp);
                Err(e)
            }
        }
    })
    .await
}

/// Nome sugerido: o nome original do documento, ou o do arquivo no celular.
/// Sem separadores de pasta (o nome vem do backup, não é confiável).
fn suggested_name(r: &MediaRef) -> String {
    let raw = r
        .file_name
        .clone()
        .or_else(|| r.file_path.as_deref().and_then(|p| p.rsplit(['/', '\\']).next()).map(str::to_owned))
        .unwrap_or_else(|| format!("midia-{}", r.message_id));
    let clean: String = raw
        .chars()
        .map(|c| if c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') { '_' } else { c })
        .collect();
    let clean = clean.trim_start_matches('.').trim();
    if clean.is_empty() { format!("midia-{}", r.message_id) } else { clean.chars().take(150).collect() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(name: Option<&str>, path: Option<&str>) -> MediaRef {
        MediaRef {
            message_id: 7,
            kind: "document".into(),
            file_hash: None,
            file_path: path.map(Into::into),
            mime: None,
            file_name: name.map(Into::into),
        }
    }

    #[test]
    fn nome_sugerido_seguro() {
        assert_eq!(suggested_name(&r(Some("contrato.pdf"), None)), "contrato.pdf");
        assert_eq!(suggested_name(&r(Some("../../etc/passwd"), None)), "_.._etc_passwd");
        assert_eq!(suggested_name(&r(None, Some("Media/WhatsApp Documents/a b.pdf"))), "a b.pdf");
        assert_eq!(suggested_name(&r(Some("..."), None)), "midia-7");
        assert_eq!(suggested_name(&r(None, None)), "midia-7");
    }
}
