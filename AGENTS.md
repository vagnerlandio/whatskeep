# Agent guide

Desktop app (**Tauri v2 + React**) that imports WhatsApp / WhatsApp Business backups (Google Drive or a local copy), decrypts them **in Rust**, stores them in an **encrypted vault** and shows the history with a WhatsApp-like UI. It can also produce an **encrypted offline HTML export**.

Read this file before changing anything. For details:
- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) — data flow, key hierarchy, on-disk formats, dependencies between modules.
- [docs/RECIPES.md](docs/RECIPES.md) — step-by-step checklists for common changes (new command, new message field, new WhatsApp message type...).
- [SECURITY.md](SECURITY.md) — threat model and limitations. [README.md](README.md) — user-facing overview. (Both in Portuguese.)

## Map

| Path | What it is | Depends on |
|---|---|---|
| `crates/wa-crypt` | Decrypts crypt14/crypt15 and `.mcrypt1` media. Pure: no I/O, no Tauri. | — |
| `crates/wa-import` | `msgstore.db` (modern and legacy schemas) → the app's normalized schema (`store.rs`), with FTS5. | — |
| `crates/wa-vault` | Vault: `vault.json` (Argon2id → DEK), one SQLCipher DB per app, media blobs, **viewer queries** (`query.rs`). | wa-import |
| `crates/wa-gdrive` | Google login (gpsoauth, unofficial) and download from `backup.googleapis.com`. | wa-crypt |
| `crates/wa-export` | Builds the encrypted HTML package (format v1 in `format.rs`). | wa-vault |
| `apps/desktop/src-tauri` | Tauri commands, state, `wamedia://` protocol, ACL. Cargo package `wa-historico`. | all crates |
| `apps/desktop/src` | App screens (vault, import, Drive, media, export). | @wa/viewer |
| `packages/viewer` | Shared WhatsApp-style UI. Takes a `DataSource`; does not know where data comes from. | — |
| `packages/export-shell` | The export's single `index.html`: decrypts in the browser (WebCrypto + Argon2id WASM). Embedded into the binary by `build.rs`. | @wa/viewer |

## Commands

```sh
pnpm install
pnpm dev                       # app in dev mode (builds export-shell first)
cargo test --workspace         # ~110 Rust tests (crypto, vault, importer, ACL, export)
cargo clippy --workspace --all-targets   # must stay at 0 warnings
pnpm lint:security             # forbidden frontend patterns (scripts/check-security.sh)
pnpm test:e2e                  # export opened via file:// in Chromium and WebKit (Playwright)
pnpm --filter desktop exec tsc --noEmit  # types (same for packages/viewer and packages/export-shell)
cargo audit && pnpm audit --prod
```

Single crate: `cargo test -p wa-vault`. Ignored perf test: `cargo test -p wa-vault --release -- --ignored --nocapture`.

## Non-negotiable rules (security)

Breaking any of these requires discussing it with the user first.

1. **All crypto in Rust.** Keys, the DEK, Google tokens and decrypted content never reach the webview. The webview only gets query results (message pages) and `wamedia://` URLs. (By-design exception: the HTML export decrypts in the browser.)
2. **No plaintext on disk.** Decrypted data goes straight into SQLCipher; media into AES-GCM blobs. Every connection holding decrypted data uses `PRAGMA temp_store = MEMORY` and `cipher_memory_security = ON` (tested). Sensitive buffers are `Zeroizing`.
3. **Google tokens live in memory only**, dropped when the download ends or the vault locks. Never persist them.
4. **Message content is hostile.** Forbidden: `dangerouslySetInnerHTML`, `innerHTML`, `eval`, dynamic `href={...}`, iframes, `localStorage`. Links go through `LinkConfirm` + the `open_link` command (http/https only, no `user@`). No link previews, no remote map tiles. `pnpm lint:security` enforces this.
5. **Never use a phone number as a key, id or file name.** Export ids are random.
6. **Never commit real backups, keys or databases.** `.gitignore` blocks `*.crypt1x`, `msgstore*.db*`, `key`, `*.hex`, ...; only the synthetic fixtures in `crates/*/tests/fixtures/` are allowed.
7. **Errors to the UI** are `CmdError { code, message }` (`src-tauri/src/error.rs`): stable `code`, `message` never contains key material or message content.
8. **Explicit ACL:** every new command must be in the `build.rs` manifest **and** in `capabilities/default.json` (see [RECIPES](docs/RECIPES.md#new-tauri-command)). The `google-login` window has no permissions at all.
9. **CSP:** production has no `data:`/`blob:` for images. In the export, **never** put `'self'` in the CSP (on WebKit, `'self'` on a `file://` page allows any remote URL) — use `file:`.

## Conventions

- **Language:** source code identifiers, code comments, test names, UI strings and user docs (README, SECURITY) are in **Portuguese** — keep it that way. Agent docs (this file, `docs/`) are in English. **Commit messages in English**, ending with the `Co-Authored-By` line the harness asks for.
- Rust edition 2024, workspace in `Cargo.toml` (rusqlite with SQLCipher across the whole workspace — plain SQLite cannot be mixed in).
- Heavy Tauri commands run inside `commands::blocking(...)` (spawn_blocking); long operations hold `state.busy()` so autolock does not fire mid-way.
- Viewer DB/media access only through `AppState::with_store` / `with_media` (opened on demand, closed on lock). Lock order: `conns` → `media` → `vault`.
- Progress to the UI via events: `import-progress`, `gdrive-progress`, `media-progress`, `export-progress`, `vault-locked`.
- Shared types: structs in `wa_vault::query` serialize as camelCase and mirror `packages/viewer/src/types.ts`. Change one, change the other.
- Tests use synthetic fixtures. UI harnesses use `playwright-core` (Chromium and WebKit are already in the local cache).
- Generated and committed: `apps/desktop/src-tauri/permissions/autogenerated/*.toml` (created by `tauri-build` from the manifest). `gen/` is not committed.

## Known pitfalls

- `tauri::generate_context!()` may only be expanded once: use `crate::context()` (generic over the runtime, also used by the ACL tests).
- Cursor pagination on `(sent_at, id)`. Search relies on messages being inserted in chronological order (`id` grows with `sent_at`); importers use `ORDER BY timestamp, _id`.
- `open_read_only` creates the temp `fts5vocab` table **before** setting `query_only`. Prefix search: last word ≥ 3 chars expands through the vocabulary (max 64 terms).
- `export-shell` must be built before `cargo build` of the app (`beforeDev/BuildCommand` already does it). Without it, export reports itself as unavailable.
- No `fetch` on `file://`: the export loads data via `<script src>` (`window.__waChunk`, `window.__waManifest`) and encrypted media via `<input webkitdirectory>` + `File.slice`.
- Playwright `page.evaluate` bypasses CSP — to test CSP, inject scripts through the page itself.
- Re-importing replaces an app's whole database (atomic: encrypted temp file + rename). The media index is separate and survives.

## Project status

Every phase of the plan is implemented and tested **with synthetic data only**. Not yet validated with real data:
- real backups (the user will test); the import report lists `unknownTypes` to help map new message types;
- Google login inside the embedded WebKit (fallback: pasting the `oauth_token` cookie);
- WhatsApp Business `client_sig` and whether its backups show up in the same listing;
- the `.mcrypt1` format (whether the GCM tag is present; name format in the metadata).

Out of scope / not done: incremental merge of backups (re-import replaces), contact names (`wa.db` is not in the Drive backup), password-only E2E backups (impossible: the key lives in WhatsApp's HSM).
