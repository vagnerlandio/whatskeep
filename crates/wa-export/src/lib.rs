//! Exporta conversas do cofre para uma pasta que abre offline em qualquer
//! navegador, protegida por senha. Formato em [`format`].
//!
//! ```text
//! <destino>/
//!   index.html            visualizador (um arquivo só) com CSP sem rede
//!   data/manifest.js      parâmetros do Argon2id + slots cifrados (um por senha)
//!   data/<id>.js          pedaços cifrados de mensagens (ids aleatórios)
//!   media/pack-000.bin    mídias cifradas (modo "cifradas")
//!   media/<id>            mídias em claro (modo "sem cifra")
//! ```
//!
//! * Uma senha libera só as conversas do seu slot. Conversas fora do export
//!   não entram em nada (nem mensagens, nem mídia, nem índice).
//! * Não há índice de busca gravado: a busca roda no navegador, só nas
//!   conversas destravadas.

pub mod format;

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use wa_vault::MediaReader;
use wa_vault::query::{self, Cursor, Direction};
use zeroize::Zeroizing;

use format::{Kdf, Key, Sealed, b64, random, random_id};

/// Mensagens por página lida do banco.
const PAGE: u32 = 500;
/// JSON acumulado antes de fechar um pedaço (~2 MiB antes do gzip).
const CHUNK_TARGET: usize = 2 << 20;
/// Tamanho máximo de um pacote de mídia.
const PACK_MAX: u64 = 1 << 30;
pub const MIN_PASSWORD_CHARS: usize = 8;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Invalid(String),
    #[error("a pasta de destino já tem arquivos; escolha uma pasta vazia")]
    DestinationNotEmpty,
    #[error("cancelado")]
    Cancelled,
    #[error("senha incorreta ou arquivo adulterado")]
    Authentication,
    #[error("formato inválido: {0}")]
    Format(&'static str),
    #[error("o sistema não forneceu números aleatórios")]
    Random,
    #[error("erro de arquivo: {0}")]
    Io(#[from] std::io::Error),
    #[error("erro no banco de dados: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    Vault(#[from] wa_vault::Error),
    #[error("JSON: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MediaMode {
    /// Só as miniaturas (dentro das mensagens cifradas).
    None,
    /// Mídias cifradas com a chave da conversa (padrão).
    Encrypted,
    /// Mídias em claro, com nomes aleatórios.
    Plain,
}

impl MediaMode {
    fn as_str(self) -> &'static str {
        match self {
            MediaMode::None => "none",
            MediaMode::Encrypted => "encrypted",
            MediaMode::Plain => "plain",
        }
    }
}

/// Conversas liberadas por uma senha.
pub struct Group {
    pub password: Zeroizing<String>,
    pub chats: Vec<i64>,
}

pub struct Options {
    pub groups: Vec<Group>,
    pub media: MediaMode,
    pub kdf: Kdf,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub chats_done: u64,
    pub chats_total: u64,
    pub messages: u64,
    pub media_files: u64,
    pub media_bytes: u64,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub chats: u64,
    pub messages: u64,
    pub media_files: u64,
    pub media_bytes: u64,
    /// Mídias citadas nas mensagens, mas que não estão no cofre.
    pub media_missing: u64,
    pub slots: u64,
}

#[derive(Serialize, Deserialize)]
pub struct SlotContent {
    pub master: String,
    pub chats: Vec<ChatEntry>,
}

#[derive(Serialize, Deserialize)]
pub struct ChatEntry {
    /// Id aleatório usado na derivação da chave da conversa.
    pub key_id: String,
    /// A conversa no formato do visualizador.
    pub chat: Value,
    pub chunks: Vec<String>,
}

/// Escreve o pacote em `dest` (vazia ou inexistente).
///
/// `shell_html` é o `index.html` do visualizador (`packages/export-shell`),
/// que recebe aqui a CSP com os hashes dos seus scripts.
pub fn export(
    conn: &Connection,
    media: Option<&MediaReader>,
    shell_html: &str,
    dest: &Path,
    opts: &Options,
    cancel: &AtomicBool,
    mut progress: impl FnMut(&Progress),
) -> Result<Summary, Error> {
    validate(opts)?;
    prepare_dest(dest)?;
    fs::create_dir_all(dest.join("data"))?;
    if opts.media != MediaMode::None {
        fs::create_dir_all(dest.join("media"))?;
    }
    if opts.media == MediaMode::Plain {
        // Spotlight (macOS) não indexa as mídias em claro.
        File::create(dest.join(".metadata_never_index"))?;
    }

    let all: HashMap<i64, query::Chat> = query::chats(conn)?.into_iter().map(|c| (c.id, c)).collect();
    for g in &opts.groups {
        for id in &g.chats {
            if !all.contains_key(id) {
                return Err(Error::Invalid(format!("conversa {id} não existe")));
            }
        }
    }

    let salt: [u8; 16] = random()?;
    let mut w = Writer {
        dest: dest.to_owned(),
        salt,
        media,
        mode: opts.media,
        pack: None,
        pack_index: 0,
        summary: Summary::default(),
        progress: Progress { chats_total: opts.groups.iter().map(|g| g.chats.len() as u64).sum(), ..Default::default() },
    };

    let mut slots = Vec::new();
    for group in &opts.groups {
        let master = Key::new(random()?);
        let mut entries = Vec::new();
        for id in &group.chats {
            if cancel.load(Ordering::SeqCst) {
                return Err(Error::Cancelled);
            }
            entries.push(w.chat(conn, &all[id], &master, cancel, &mut progress)?);
            w.progress.chats_done += 1;
            progress(&w.progress);
        }
        let content = Zeroizing::new(serde_json::to_vec(&SlotContent { master: b64(master.as_ref()), chats: entries })?);
        let key = format::password_key(&group.password, &salt, opts.kdf)?;
        let (ct, nonce) = format::seal(&key, format::SLOT_AAD, &content)?;
        slots.push(Sealed { n: b64(&nonce), c: b64(&ct) });
    }
    w.close_pack()?;

    let manifest = format::Manifest {
        format: format::FORMAT,
        kdf: format::KdfJson {
            alg: "argon2id".into(),
            m: opts.kdf.m,
            t: opts.kdf.t,
            p: opts.kdf.p,
            salt: b64(&salt),
        },
        slots,
        media: opts.media.as_str().into(),
    };
    fs::write(
        dest.join("data/manifest.js"),
        format!("window.__waManifest = {};\n", serde_json::to_string(&manifest)?),
    )?;
    fs::write(dest.join("index.html"), with_csp(shell_html))?;

    let mut summary = w.summary;
    summary.slots = opts.groups.len() as u64;
    Ok(summary)
}

fn validate(opts: &Options) -> Result<(), Error> {
    if opts.groups.is_empty() || opts.groups.iter().all(|g| g.chats.is_empty()) {
        return Err(Error::Invalid("escolha pelo menos uma conversa".into()));
    }
    let mut seen = std::collections::HashSet::new();
    for g in &opts.groups {
        if g.password.chars().count() < MIN_PASSWORD_CHARS {
            return Err(Error::Invalid(format!("as senhas precisam ter pelo menos {MIN_PASSWORD_CHARS} caracteres")));
        }
        for id in &g.chats {
            if !seen.insert(*id) {
                return Err(Error::Invalid("uma conversa só pode estar em uma senha".into()));
            }
        }
    }
    Ok(())
}

fn prepare_dest(dest: &Path) -> Result<(), Error> {
    if dest.exists() {
        let mut entries = fs::read_dir(dest)?;
        if entries.any(|e| e.is_ok_and(|e| e.file_name() != ".DS_Store")) {
            return Err(Error::DestinationNotEmpty);
        }
    } else {
        fs::create_dir_all(dest)?;
    }
    Ok(())
}

struct Writer<'a> {
    dest: PathBuf,
    salt: [u8; 16],
    media: Option<&'a MediaReader>,
    mode: MediaMode,
    pack: Option<(BufWriter<File>, u64)>,
    pack_index: u32,
    summary: Summary,
    progress: Progress,
}

impl Writer<'_> {
    fn chat(
        &mut self,
        conn: &Connection,
        chat: &query::Chat,
        master: &Key,
        cancel: &AtomicBool,
        progress: &mut impl FnMut(&Progress),
    ) -> Result<ChatEntry, Error> {
        let key_id = random_id()?;
        let key = format::chat_key(master, &self.salt, &key_id);
        let mut chunks = Vec::new();
        let mut buf: Vec<Value> = Vec::new();
        let mut buf_size = 0usize;
        // Mesma mídia em várias mensagens da conversa: grava uma vez.
        let mut stored: HashMap<String, Value> = HashMap::new();
        let mut cursor: Option<Cursor> = None;

        loop {
            if cancel.load(Ordering::SeqCst) {
                return Err(Error::Cancelled);
            }
            let page = query::messages(conn, chat.id, Direction::After, cursor, PAGE)?;
            let Some(last) = page.last() else { break };
            cursor = Some(Cursor { sent_at: last.sent_at, id: last.id });
            let full = page.len() == PAGE as usize;

            for m in page {
                let mut v = serde_json::to_value(&m)?;
                if let Some(media) = &m.media {
                    let mut extra = serde_json::Map::new();
                    if media.has_thumbnail
                        && let Some(t) = query::thumbnail(conn, m.id)? {
                            extra.insert("thumb".into(), Value::String(b64(&t)));
                        }
                    let r = if m.kind == "deleted" { None } else { self.media_ref(media, &key, &mut stored)? };
                    if r.is_none() && media.file_path.is_some() && m.kind != "deleted" {
                        self.summary.media_missing += 1;
                    }
                    let available = r.is_some();
                    if let Some(r) = r {
                        extra.insert("ref".into(), r);
                    }
                    if let Some(obj) = v.get_mut("media").and_then(Value::as_object_mut) {
                        obj.insert("available".into(), Value::Bool(available));
                        obj.extend(extra);
                    }
                }
                strip_nulls(&mut v);
                buf_size += v.to_string().len();
                buf.push(v);
                self.summary.messages += 1;
                self.progress.messages += 1;
                if buf_size >= CHUNK_TARGET {
                    chunks.push(self.write_chunk(&key, &mut buf)?);
                    buf_size = 0;
                    progress(&self.progress);
                }
            }
            if !full {
                break;
            }
        }
        if !buf.is_empty() || chunks.is_empty() {
            chunks.push(self.write_chunk(&key, &mut buf)?);
        }
        self.summary.chats += 1;
        Ok(ChatEntry { key_id, chat: serde_json::to_value(chat)?, chunks })
    }

    fn write_chunk(&mut self, key: &Key, buf: &mut Vec<Value>) -> Result<String, Error> {
        let id = random_id()?;
        let json = Zeroizing::new(serde_json::to_vec(&json!({ "messages": buf }))?);
        buf.clear();
        let packed = format::pack_chunk(&json)?;
        let (ct, nonce) = format::seal(key, id.as_bytes(), &packed)?;
        let mut blob = nonce.to_vec();
        blob.extend_from_slice(&ct);
        fs::write(
            self.dest.join("data").join(format!("{id}.js")),
            format!("window.__waChunk(\"{id}\",\"{}\");\n", b64(&blob)),
        )?;
        Ok(id)
    }

    /// Copia a mídia do cofre para o pacote e devolve a referência gravada na mensagem.
    fn media_ref(
        &mut self,
        media: &query::Media,
        chat_key: &Key,
        stored: &mut HashMap<String, Value>,
    ) -> Result<Option<Value>, Error> {
        if self.mode == MediaMode::None {
            return Ok(None);
        }
        let Some(reader) = self.media else { return Ok(None) };
        let Some(blob) = reader.find(media.file_hash.as_deref(), media.file_path.as_deref())? else {
            return Ok(None);
        };
        if let Some(r) = stored.get(&blob.sha256) {
            return Ok(Some(r.clone()));
        }
        let mime = blob.mime.clone().or_else(|| media.mime.clone());
        let r = match self.mode {
            MediaMode::Plain => {
                let name = random_id()?;
                let f = BufWriter::new(File::create(self.dest.join("media").join(&name))?);
                reader.copy_to(&blob, f)?;
                json!({ "f": format!("media/{name}"), "mime": mime, "n": blob.size })
            }
            MediaMode::Encrypted => {
                let len = format::stream_len(blob.size);
                if self.pack.as_ref().is_some_and(|(_, off)| *off > 0 && off + len > PACK_MAX) {
                    self.close_pack()?;
                    self.pack_index += 1;
                }
                if self.pack.is_none() {
                    let path = self.dest.join("media").join(format!("pack-{:03}.bin", self.pack_index));
                    self.pack = Some((BufWriter::new(File::create(path)?), 0));
                }
                let (file, offset) = self.pack.as_mut().expect("aberto acima");
                let media_id = random_id()?;
                let prefix: [u8; 7] = random()?;
                let key = format::media_key(chat_key, &self.salt, &media_id);
                let mut sw = format::StreamWriter::new(file, &key, prefix, &media_id, blob.size);
                reader.copy_to(&blob, &mut sw)?;
                sw.finish()?;
                let r = json!({
                    "p": self.pack_index, "o": *offset, "n": blob.size,
                    "id": media_id, "np": b64(&prefix), "mime": mime,
                });
                *offset += len;
                r
            }
            MediaMode::None => unreachable!(),
        };
        self.summary.media_files += 1;
        self.summary.media_bytes += blob.size;
        self.progress.media_files += 1;
        self.progress.media_bytes += blob.size;
        stored.insert(blob.sha256, r.clone());
        Ok(Some(r))
    }

    fn close_pack(&mut self) -> Result<(), Error> {
        if let Some((mut f, _)) = self.pack.take() {
            f.flush()?;
            f.into_inner().map_err(|e| e.into_error())?.sync_all()?;
        }
        Ok(())
    }
}

/// Remove campos `null`, `false` e listas vazias (o visualizador assume os padrões).
fn strip_nulls(v: &mut Value) {
    if let Value::Object(map) = v {
        map.retain(|_, x| !matches!(x, Value::Null | Value::Bool(false)) && !matches!(x, Value::Array(a) if a.is_empty()));
        for x in map.values_mut() {
            strip_nulls(x);
        }
    }
}

/// Insere no `<head>` uma CSP que só permite os scripts do próprio pacote
/// (os inline pelo hash), proíbe qualquer conexão de rede e formulários.
pub fn with_csp(html: &str) -> String {
    use base64::Engine;
    let mut hashes = Vec::new();
    let mut rest = html;
    while let Some(start) = rest.find("<script") {
        let tag_end = match rest[start..].find('>') {
            Some(i) => start + i + 1,
            None => break,
        };
        let tag = &rest[start..tag_end];
        let Some(close) = rest[tag_end..].find("</script>") else { break };
        if !tag.contains(" src=") {
            let body = &rest[tag_end..tag_end + close];
            hashes.push(format!(
                "'sha256-{}'",
                base64::engine::general_purpose::STANDARD.encode(Sha256::digest(body.as_bytes()))
            ));
        }
        rest = &rest[tag_end + close..];
    }
    // Sem 'self': no WebKit (Safari), 'self' numa página file:// casa com
    // QUALQUER URL, inclusive https remoto. `file:` libera só arquivos locais
    // (os dados ao lado do index.html) nos dois motores.
    let csp = format!(
        "default-src 'none'; script-src file: 'wasm-unsafe-eval' {}; style-src 'unsafe-inline'; \
         img-src file: blob: data:; media-src file: blob:; connect-src 'none'; \
         object-src 'none'; frame-src 'none'; base-uri 'none'; form-action 'none'",
        hashes.join(" ")
    );
    let meta = format!(
        "<meta http-equiv=\"Content-Security-Policy\" content=\"{csp}\">\n<meta name=\"referrer\" content=\"no-referrer\">\n"
    );
    match html.find("<head>") {
        Some(i) => format!("{}{meta}{}", &html[..i + 6], &html[i + 6..]),
        None => format!("{meta}{html}"),
    }
}
