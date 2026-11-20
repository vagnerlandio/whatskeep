# Architecture

Implementer's view. For users, see the [README](../README.md); for threats, [SECURITY.md](../SECURITY.md).

## Data flow

```text
                        ┌──────────── Google Drive (wk-gdrive) ─────────────┐
 oauth_token cookie ──► master token ──► drive.appdata access token (1 h, RAM only)
                        └──► backup.googleapis.com ─► msgstore.db.crypt15 (RAM, MD5 checked)
                                                     └► Media/... or <hash>.mcrypt1
 local .crypt14/.crypt15 file ──┐
                                ▼
             wk-crypt::decrypt(bytes, BackupKey) ─► SQLite bytes in RAM (Zeroizing)
                                ▼
             wk-import::open_msgstore (sqlite3_deserialize, temp_store=MEMORY)
                                ▼
             wk-vault StoreHandle::import ─► wk-import::import ─► stores/<app>.db (SQLCipher)
                                                                 (encrypted temp + rename)
 media folder / Drive media ─► MediaWriter::put ─► stores/<app>-media/<id> (STREAM AES-GCM)
                                               └► stores/<app>-media.db (SQLCipher index)
                                ▼
             wk_vault::query (chats, messages, around, search, thumbnail, media_ref)
                 │                                         │
   Tauri commands (JSON over IPC) + wkmedia://       wk-export::export
                 ▼                                         ▼
   packages/viewer  ◄── DataSource ──►  packages/export-shell (BundleSource, in the browser)
   (tauriSource in apps/desktop/src/api.ts)
```

## Key hierarchy

### Vault (`crates/wk-vault`)

```text
password ──Argon2id(256 MiB, t=3, p=4, salt)──► KEK ──AES-256-GCM (AAD = KDF params)──► DEK (random)
DEK ──HKDF(salt = vault_id, "wk-vault store v2:<app>")──► SQLCipher key of the message DB
DEK ──HKDF(salt = vault_id, "wk-vault media v2:<app>")──► media blob key
DEK ──HKDF(salt = vault_id, "wk-vault media-index v2:<app>")──► SQLCipher key of the media index
```

`<app>` = `whatsapp` | `business` (`SourceApp::as_str`). Changing the password only re-wraps the DEK. SQLCipher is opened with a raw key (`PRAGMA key = "x'..'"`).

### WhatsApp backups (`crates/wk-crypt`)

- **crypt15**: 32-byte root key (64 hex digits or `encrypted_backup.key`) → two-step HMAC-SHA256 with `"backup encryption"` → AES-256-GCM; the protobuf header carries the IV; content is zlib → SQLite.
- **crypt14**: key from the `key` file (needs root on the phone); IV at an offset in the header.
- **mcrypt1** (E2E media on Drive): metadata `[16][iv][32][mac][AES-CBC]` (keys via HKDF with "metadata authentication" / "metadata encryption"); content is AES-GCM with key+IV = HKDF(root, info = hex bytes of the file name, 48 B). Without a valid tag, the result is accepted only if SHA-256 == the msgstore's `file_hash`.

### Export (`crates/wk-export/src/format.rs`)

```text
password ──Argon2id(64 MiB, t=3, p=1, export-wide salt)──► slot key ──AES-GCM("wk-export slot v2")──► { master, chats }
master ──HKDF(salt, "wk-export chat v2:" ‖ id)──► chat key
chat key ──HKDF(salt, "wk-export media v2:" ‖ media id)──► media key
```

One Argon2id per password attempt; slots are tried with GCM only. The mirror JS implementation is in `packages/export-shell/src/crypto.ts` and `bundle.ts` — **any format change must be made on both sides** and covered by the e2e test.

## On-disk formats

| File | Format |
|---|---|
| `vault/vault.json` | JSON: Argon2id params, salt, vault_id, wrapped DEK |
| `vault/stores/<app>.db` | SQLCipher with the schema in `wk-import/src/store.rs` (`SCHEMA_VERSION = 1`) |
| `vault/stores/<app>-media.db` | SQLCipher: `blob(sha256 PK, id, size, mime)`, `path(path PK, sha256)` |
| `vault/stores/<app>-media/<id>` | STREAM AES-256-GCM, 1 MiB segments, nonce = prefix7 ‖ u32 BE counter ‖ last flag, AAD = id |
| export `data/manifest.js` | `window.__wkManifest = {format, kdf, slots[], media}` |
| export `data/<id>.js` | `window.__wkChunk(id, b64)`; plaintext = `[u32 LE len][gzip JSON {messages:[...]}][zero pad to 64 KiB]` |
| export `media/pack-NNN.bin` | concatenated STREAM media (up to 1 GB per pack) |

Messages in the export are the JSON of `wk_vault::query::Message` with null/false fields stripped (`strip_nulls`), plus `media.thumb` (b64), `media.ref` and `media.available`; `normalizeMessage` in `bundle.ts` restores the defaults.

## Normalized schema (summary)

`contact(jid, phone, name)`, `chat(contact_id, kind, title, archived, hidden, message_count, last_message_at)`, `message(chat_id, key_id, from_me, sender_id, sent_at ms, kind, wa_type, text, starred, edited, system_action, quoted_*)`, `media`, `thumbnail`, `location`, `vcard`, `reaction`, `message_fts` (FTS5 `unicode61 remove_diacritics 2`, content=message), `meta`. Source of truth: `crates/wk-import/src/store.rs`.

WhatsApp `message_type` → `MessageKind` mapping: `crates/wk-import/src/kind.rs`. Unknown types become `unknown` and are counted in `ImportReport.unknown_types`.

## Tauri app (`apps/desktop/src-tauri/src`)

| File | Role |
|---|---|
| `lib.rs` | Builder: plugins (dialog, opener, nav-guard), `wkmedia` protocol, `invoke_handler`, `context()`, ACL and navigation tests |
| `state.rs` | `AppState` (vault, read-only connections, media readers, password backoff, 5 min autolock, Drive session, export) |
| `commands.rs` | Vault, local import, viewer (`list_chats`, `get_messages`, `get_messages_around`, `search_messages`) |
| `gdrive.rs` | `google-login` window (ephemeral, no IPC), cookie capture, backup download job (database → import → other files → media import) |
| `backup_folder.rs` | The backup copy in the user's folder: safe relative paths, `.whatskeep-drive.json` manifest (sizes + encrypted `.mcrypt1` metadata), resumable parallel download (`.partial` + rename after MD5) |
| `media.rs` | `wkmedia://localhost/{thumb,media}/<app>/<message_id>` handler with Range, `no-store`, closed MIME list; serves only the `main` window |
| `media_import.rs` | Import a media folder (the phone's, or a saved Drive backup, `.mcrypt1` with the key), cancel, save media (native picker in the backend) |
| `export.rs` | Pick destination (native picker), run export, cancel |
| `links.rs` | `validate` + `open_link` (http/https, no userinfo) |
| `error.rs` | `CmdError` and conversions from the crates' errors |
| `build.rs` | Embeds `export-shell/dist/index.html`; command manifest (ACL) |

Frontend: `apps/desktop/src/api.ts` (`invoke` wrappers + `tauriSource(store)`), `App.tsx` (vault state / routing), `jobs.tsx` (the app-wide import/download job and its progress banner), `screens/*`.

## Shared viewer (`packages/viewer/src`)

`Viewer.tsx` (layout) → `ChatList`, `ChatPane` (react-virtuoso, provides `ViewerContext`), `MessageRow`, `MediaView` (thumbnail/lightbox/audio), `SearchPanel`, `LinkConfirm`. `format.tsx` does WhatsApp formatting (`*bold*`, `_italic_`, links) producing React nodes — never HTML. `source.ts` defines `DataSource`; implementations: `tauriSource` (app) and `BundleSource` (export).
