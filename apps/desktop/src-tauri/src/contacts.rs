//! Contact names from a Google Contacts export (CSV or vCard).
//!
//! The file is only read: the normalized numbers and display names go into the
//! vault's encrypted address book and are written into every history (and
//! again after each re-import). Names and numbers never reach the log, only
//! counts.

use std::collections::HashMap;

use serde::Serialize;
use tauri::{AppHandle, Manager};
use wk_import::SourceApp;
use wk_import::contacts::{infer_country_code, normalize, parse};

use crate::commands::blocking;
use crate::error::CmdError;
use crate::state::AppState;

/// Larger than any real address book export; refuses the wrong file early.
const MAX_FILE: u64 = 64 << 20;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContactsInfo {
    people: u64,
    numbers: u64,
    imported_at_ms: i64,
    /// One-to-one chats in the vault that show a name.
    named_chats: u64,
}

fn named_chats(state: &AppState) -> Result<u64, CmdError> {
    let mut total = 0;
    for app in SourceApp::ALL {
        let n = state.with_store(app, |c| {
            Ok(c.query_row(
                "SELECT count(*) FROM chat c JOIN contact ct ON ct.id = c.contact_id
                 WHERE c.kind = 'direct' AND ct.name IS NOT NULL
                   AND EXISTS (SELECT 1 FROM message m WHERE m.chat_id = c.id AND m.kind <> 'system')",
                [],
                |r| r.get::<_, i64>(0),
            )?)
        });
        match n {
            Ok(n) => total += n as u64,
            Err(e) if e.code == "no_store" => {}
            Err(e) => return Err(e),
        }
    }
    Ok(total)
}

fn info(state: &AppState) -> Result<Option<ContactsInfo>, CmdError> {
    let book = state.vault().as_ref().ok_or_else(CmdError::locked)?.address_book()?;
    let Some(i) = book.info()? else { return Ok(None) };
    Ok(Some(ContactsInfo { people: i.people, numbers: i.numbers, imported_at_ms: i.imported_at_ms, named_chats: named_chats(state)? }))
}

#[tauri::command]
pub async fn contacts_info(app: AppHandle) -> Result<Option<ContactsInfo>, CmdError> {
    blocking("contacts_info", move || info(&app.state::<AppState>())).await
}

/// Reads the export at `path`, keeps its names in the vault and shows them in
/// every history.
#[tauri::command]
pub async fn contacts_import(app: AppHandle, path: String) -> Result<ContactsInfo, CmdError> {
    blocking("contacts_import", move || {
        let state = app.state::<AppState>();
        let _job = state.start_job()?;
        let size = std::fs::metadata(&path)?.len();
        if size > MAX_FILE {
            return Err(CmdError::new("contacts_unrecognized", "este arquivo não parece uma exportação de contatos"));
        }
        let people = parse(&std::fs::read(&path)?);
        if people.is_empty() {
            tracing::warn!(bytes = size, "contacts: no person with a name and a number in the file");
            return Err(CmdError::new(
                "contacts_unrecognized",
                "nenhum contato com nome e telefone encontrado: exporte do Google Contatos em CSV do Google ou vCard",
            ));
        }

        // Numbers saved without a country code get the history's most common one.
        let mut phones: Vec<String> = Vec::new();
        for app in SourceApp::ALL {
            match state.with_store(app, |c| {
                Ok(c.prepare("SELECT phone FROM contact WHERE phone IS NOT NULL")?
                    .query_map([], |r| r.get::<_, String>(0))?
                    .collect::<Result<Vec<_>, _>>()?)
            }) {
                Ok(p) => phones.extend(p),
                Err(e) if e.code == "no_store" => {}
                Err(e) => return Err(e),
            }
        }
        let cc = infer_country_code(phones.iter().map(String::as_str));

        let mut numbers: HashMap<String, String> = HashMap::new();
        let mut kept_people = 0u64;
        let mut skipped_numbers = 0u64;
        for p in &people {
            let mut any = false;
            for raw in &p.phones {
                match normalize(raw, cc.as_deref()) {
                    Some(n) => {
                        // The first person with a number keeps it.
                        numbers.entry(n).or_insert_with(|| p.name.clone());
                        any = true;
                    }
                    None => skipped_numbers += 1,
                }
            }
            kept_people += u64::from(any);
        }

        let map = {
            let book = state.vault().as_ref().ok_or_else(CmdError::locked)?.address_book()?;
            book.replace(&numbers, kept_people)?;
            book.load()?
        };
        let mut applied = (0u64, 0u64);
        for app in SourceApp::ALL {
            let a = apply(&state, app, map.as_ref())?;
            applied.0 += a.contacts;
            applied.1 += a.chats;
        }
        tracing::info!(
            people_in_file = people.len(),
            people_kept = kept_people,
            numbers = numbers.len(),
            numbers_skipped = skipped_numbers,
            country_code_known = cc.is_some(),
            contacts_named = applied.0,
            chats_named = applied.1,
            "contacts imported"
        );
        info(&state)?.ok_or_else(|| CmdError::new("internal", "a agenda não foi gravada"))
    })
    .await
}

/// Removes the address book and the names from every history.
#[tauri::command]
pub async fn contacts_remove(app: AppHandle) -> Result<(), CmdError> {
    blocking("contacts_remove", move || {
        let state = app.state::<AppState>();
        let _job = state.start_job()?;
        state.vault().as_ref().ok_or_else(CmdError::locked)?.address_book()?.delete()?;
        for app in SourceApp::ALL {
            apply(&state, app, None)?;
        }
        tracing::info!("contacts removed");
        Ok(())
    })
    .await
}

/// Writes the names into one history (closing the viewer's connection to it,
/// which would keep showing the old names).
fn apply(state: &AppState, app: SourceApp, map: Option<&HashMap<String, String>>) -> Result<wk_vault::NamesApplied, CmdError> {
    let store = state.vault().as_ref().ok_or_else(CmdError::locked)?.store(app)?;
    state.forget_store(app);
    let applied = store.apply_names(map);
    state.forget_store(app);
    Ok(applied?)
}

/// After a backup import: puts the address book's names into the new history.
/// A failure here does not undo the import (it is only logged).
pub(crate) fn apply_after_import(state: &AppState, app: SourceApp) {
    let result = (|| -> Result<Option<wk_vault::NamesApplied>, CmdError> {
        let map = state.vault().as_ref().ok_or_else(CmdError::locked)?.address_book()?.load()?;
        match map {
            Some(map) => apply(state, app, Some(&map)).map(Some),
            None => Ok(None),
        }
    })();
    match result {
        Ok(Some(a)) => tracing::info!(app = app.as_str(), contacts_named = a.contacts, chats_named = a.chats, "contact names applied"),
        Ok(None) => {}
        Err(e) => tracing::warn!(app = app.as_str(), code = e.code, "could not apply the contact names: {}", e.message),
    }
}
