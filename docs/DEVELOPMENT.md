# Development

Day-to-day details for working on WhatsKeep. Conventions and rules: [AGENTS.md](../AGENTS.md). Formats: [ARCHITECTURE.md](ARCHITECTURE.md). Step-by-step changes: [RECIPES.md](RECIPES.md).

## Setup

Requirements: macOS with the Xcode Command Line Tools, Rust (latest stable), Node.js 20.19+ or 22.12+ and pnpm 10.

```sh
pnpm install
pnpm dev          # development app (builds the export shell first)
pnpm dev:log      # same, teeing the diagnostics log to logs/dev-<timestamp>.log
pnpm build        # release installer in target/release/bundle/
```

The first Rust build compiles SQLCipher and OpenSSL from source and takes a few minutes. `pnpm dev` recompiles and restarts the app whenever a Rust file changes, which locks the vault.

## Tests

```sh
cargo test --workspace                 # all Rust tests
cargo test -p wk-vault                 # one crate
cargo test -p wk-vault --release -- --ignored --nocapture   # ignored performance tests
cargo clippy --workspace --all-targets # must stay at 0 warnings
pnpm lint:security                     # forbidden frontend patterns (scripts/check-security.sh)
pnpm --filter desktop exec tsc --noEmit
pnpm test:e2e                          # export end-to-end in Chromium and WebKit
```

### End-to-end export test

`pnpm test:e2e` (`packages/export-shell/e2e/run.sh`):

1. builds `packages/export-shell`;
2. runs the ignored Rust test `generate_browser_packages` (`crates/wk-export/tests/export.rs`) with `WK_EXPORT_OUT`, `WK_EXPORT_SHELL` and `WK_EXPORT_IMAGE`, which writes three export packages (encrypted media, plain media, one password per chat);
3. opens each one via `file://` in Chromium and WebKit with `playwright-core` (`e2e/export.e2e.cjs`): unlocking, search, media, password isolation, language switch, and a network probe that must never reach the network.

The package names and passwords in the generator must match the ones the e2e script uses.

### UI harness

The desktop UI can be exercised without the Rust backend by loading the Vite dev server (`http://localhost:1420`) in Playwright with an init script that mocks `window.__TAURI_INTERNALS__.invoke` and returns synthetic data. `page.evaluate` bypasses CSP, so CSP checks must run scripts from inside the page.

## Test fixtures

Only synthetic data is committed (`crates/*/tests/fixtures/`); never commit real backups, keys or databases (`.gitignore` blocks them).

The crypt14/crypt15 vectors were produced with an independent open-source implementation of the format (wa-crypt-tools 0.1.0), with random keys and a fictitious SQLite database, so our decryption is checked against code we did not write:

```sh
waencrypt --type 15 "$(cat key15.hex)" plain.db msgstore.db.crypt15
wacreatekey --hex "$(cat key15.hex)"          # → encrypted_backup.key
wacreatekey -c14 --hex <random hex>           # → key
waencrypt --type 14 key plain.db msgstore.db.crypt14
```

`crates/wk-import/tests/fixtures/{modern,legacy}.sql` build msgstore databases for both WhatsApp schemas; the Portuguese message text in them is test data that several tests search for.

## Diagnostics log

`apps/desktop/src-tauri/src/diag.rs`, via `tracing`:

- on in debug builds; in release only with `WK_LOG=<filter>` (e.g. `WK_LOG=debug`); stderr only;
- default filter `info,whatskeep_lib=debug,wk_import=debug,wk_gdrive=debug`;
- every command logs its name, duration and result or error code; `diag::Tally` counts per-item failures by reason;
- external HTTP requests and responses are logged by `wk_gdrive::trace`, redacted (`<redacted:N>`, `<text:N>`, `<backup>`, `<n>`); `crates/wk-gdrive/tests/mock_drive.rs` fails if a secret reaches the log;
- frontend errors arrive through the `ui_log` command.

Never log message text, phone numbers, JIDs, names, passwords, keys, tokens, cookies, e-mail, backup ids or media file names.
