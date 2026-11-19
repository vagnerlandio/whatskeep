//! Preferências do app que precisam existir antes de destravar o cofre (hoje,
//! só o idioma). Ficam em `settings.json` na pasta de dados do app, fora do
//! cofre: não há nada sensível aqui.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use crate::error::CmdError;

const FILE: &str = "settings.json";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Lang {
    #[serde(rename = "pt-BR")]
    Pt,
    #[serde(rename = "en")]
    En,
}

#[derive(Default, Serialize, Deserialize)]
struct Stored {
    #[serde(default)]
    language: Option<Lang>,
}

pub struct Settings {
    path: PathBuf,
    language: Mutex<Option<Lang>>,
}

impl Settings {
    pub fn load(dir: &Path) -> Self {
        let path = dir.join(FILE);
        let stored: Stored = std::fs::read(&path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        Self { path, language: Mutex::new(stored.language) }
    }

    pub fn language(&self) -> Option<Lang> {
        *self.language.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn set_language(&self, lang: Lang) -> std::io::Result<()> {
        *self.language.lock().unwrap_or_else(|e| e.into_inner()) = Some(lang);
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(&Stored { language: Some(lang) })?)?;
        std::fs::rename(&tmp, &self.path)
    }
}

/// Texto no idioma escolhido (para o que o backend mostra: seletores nativos,
/// título de janela). Sem escolha salva, português.
pub fn tr(app: &AppHandle, pt: &'static str, en: &'static str) -> &'static str {
    match app.state::<Settings>().language() {
        Some(Lang::En) => en,
        _ => pt,
    }
}

/// Idioma salvo, ou `null` (o frontend então usa o do sistema).
#[tauri::command]
pub fn get_language(settings: State<'_, Settings>) -> Option<Lang> {
    settings.language()
}

#[tauri::command]
pub fn set_language(settings: State<'_, Settings>, language: Lang) -> Result<(), CmdError> {
    tracing::info!(idioma = ?language, "idioma alterado");
    settings.set_language(language).map_err(CmdError::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn salva_e_le_o_idioma() {
        let dir = tempfile::tempdir().unwrap();
        let s = Settings::load(dir.path());
        assert_eq!(s.language(), None);
        s.set_language(Lang::En).unwrap();
        assert_eq!(Settings::load(dir.path()).language(), Some(Lang::En));
        // Arquivo corrompido: volta ao padrão sem falhar.
        std::fs::write(dir.path().join(FILE), b"lixo").unwrap();
        assert_eq!(Settings::load(dir.path()).language(), None);
    }
}
