# Security

*Resumo em português no [final deste arquivo](#português).*

WhatsKeep handles someone's entire chat history. This document says what it protects against, how, what is out of scope, and how to verify it.

## What it protects against

| Threat | Protection |
|---|---|
| Someone with access to the disk (stolen computer, disk backup, another account) | Everything imported is encrypted in the vault (SQLCipher + AES-256-GCM), with a key derived from the password by Argon2id. No plaintext is ever written. |
| Someone at the unlocked computer | Locks after 5 minutes without use and with the "Lock" button; the key is zeroed in memory. Changing the password requires the current one. Repeated wrong passwords delay further attempts (up to 30 s). |
| Malicious message (HTML, script, deceptive link) | Message text is always rendered as text. Links open only after a confirmation, only http/https, never `user@host`. CSP without network. |
| A remote page gaining access to the app | The main window only navigates inside the app; app commands are allowed one by one, only for that window (ACL tested at runtime). The Google sign-in window has no permissions at all. |
| Leaks through auxiliary files | `temp_store = MEMORY` on every connection (including the decrypted msgstore), no core dumps, vault excluded from Time Machine and Spotlight, media saved out of the vault only by explicit action. |
| Leaks through the diagnostics log | The log is off in release builds unless `WK_LOG` is set, goes only to stderr, and never contains message text, phone numbers, names, passwords, keys, tokens, cookies, e-mail or backup ids. External requests are logged redacted; a test runs the full Google flow against a mock server and fails if any secret reaches the log. |
| Google account takeover | Google's master token exists for an instant and is zeroed; only 1-hour access tokens stay in memory, until the download ends. The app asks only for `drive.appdata`, but Google issues the token with the scopes already granted to WhatsApp (`drive.appdata`, `drive.file`, `userinfo.email`, `userinfo.profile`, `openid`; seen in a real test in 2026-11): still no access to the whole Drive, only to files created by WhatsApp. Nothing is written to disk. Sign-in happens in an incognito window. |
| An HTML export falling into the wrong hands | Messages are always encrypted (Argon2id + AES-GCM, per-chat key). One password neither opens nor lists another password's chats. Random file names, no phone number or plaintext. The page cannot access the network. |

## Details

### Vault (`crates/wk-vault`)
- `vault.json`: Argon2id (256 MiB, t=3, p=4) → key that unwraps the DEK (AES-256-GCM). The KDF parameters are part of the AAD: lowering them by editing the file makes opening fail.
- One SQLCipher database per source app, key = HKDF(DEK, vault id, label). Raw key (no SQLCipher PBKDF2), `cipher_memory_security = ON`.
- Media: one file per item, 1 MiB AES-GCM segments (STREAM: nonce with counter and last flag; AAD = file id). Detects tampering, swapping and truncation.
- Folders 0700, files 0600, `.metadata_never_index`, Time Machine exclusion (`tmutil isexcluded` confirms).
- `settings.json` (UI language only) lives outside the vault, because it is needed before unlocking; it holds nothing sensitive.

### App (`apps/desktop`)
- All cryptography in Rust; the webview only receives message pages and `wkmedia://` URLs (media served from the vault with `Cache-Control: no-store` and a closed list of MIME types).
- Production CSP: `default-src 'self'`, `connect-src ipc:`, images/media only from the app and `wkmedia:`, no `data:`/`blob:`.
- Capabilities: only `event:listen/unlisten`, `dialog:open` and the app's own commands (manifest in `build.rs`). No `core:webview:default` (which includes opening devtools) and no `core:window:default`.
- Devtools are not part of release builds (`devtools` feature off).
- Prototypes are frozen (`Object.freeze(Object.prototype)`) in the main window, as the first import of `main.tsx`. Tauri's `freezePrototype` is off because it applies to every window and breaks the JavaScript of Google's sign-in page; `pnpm lint:security` checks the import.
- File paths to write to (export, save media) are chosen in the native picker by the backend; the webview never says where to write.
- `panic = "abort"`, stripped binary.

### HTML export (`crates/wk-export`, `packages/export-shell`)
- Format in `crates/wk-export/src/format.rs`.
- CSP injected with the hashes of the inline scripts and `connect-src 'none'`. **No `'self'`**: in WebKit, `'self'` on a `file://` page allows any remote URL (verified: it fetched an image and a script from an external server). With `file:`, neither engine reaches the network.
- Locks (reloads and forgets the keys) after 15 minutes without use.

## Known limitations

- **Malware running as your user** while the vault is unlocked can read the app's memory or capture the screen. No ordinary desktop app solves this.
- **Weak password**: whoever copies `vault.json` (or an export) can try passwords offline. Argon2id makes each attempt expensive, but a long password is what really protects you. There is no password recovery.
- **Passwords and the 64-digit key pass through the webview** while typed (unavoidable in a web UI); in Rust they live in zeroed buffers.
- **System memory**: SQLCipher and Rust zero what they free, but the OS may have paged memory to swap (swap is encrypted on macOS).
- **SSD/APFS**: deleted files are not really overwritten; that is why no plaintext is written in the first place.
- **Unofficial Google flow**: it may break or be blocked at any time; local import keeps working.
- **Windows/Linux**: the system backup exclusion only exists on macOS.
- `cargo audit`: no vulnerabilities; 6 warnings about unmaintained/unsound crates in Tauri's Linux layer (GTK/glib, D-Bus), with no fix available and not part of the macOS build.

## How to verify

```sh
cargo test --workspace          # crypto, vault, isolation, tampering, ACL, leaks
pnpm test:e2e                   # export opened via file:// in Chromium and WebKit, including a network probe
pnpm lint:security              # forbidden frontend patterns (innerHTML, eval, dynamic href...)
cargo audit && pnpm audit --prod
```

Tests worth highlighting:
- `crates/wk-vault/tests/vault.rs`: no text, phone number, schema or password in plaintext in the vault files; Time Machine exclusion.
- `crates/wk-vault/tests/tempfiles.rs` and `crates/wk-import/tests/import.rs`: `temp_store = MEMORY` on every connection with decrypted data.
- `crates/wk-export/tests/export.rs`: isolation between passwords, no plaintext in the package, tampering detected, CSP without `'self'`.
- `crates/wk-gdrive/tests/mock_drive.rs`: full Google flow against a mock server; the diagnostics log contains no secret.
- `apps/desktop/src-tauri/src/lib.rs`: the real ACL at runtime (only the main window, with the app origin, can call commands).

---

## Português

O WhatsKeep lida com o histórico inteiro de conversas de alguém. Em resumo:

- **Disco:** tudo o que é importado fica cifrado no cofre (SQLCipher + AES-256-GCM, chave derivada da senha por Argon2id). Nada em claro é gravado em disco.
- **Computador destravado:** o cofre trava após 5 minutos sem uso e no botão "Travar"; senhas erradas seguidas atrasam as próximas tentativas.
- **Mensagens maliciosas:** o texto é sempre exibido como texto; links só abrem depois de confirmação e só http/https; a interface não acessa a rede.
- **Janelas e comandos:** a janela principal só navega dentro do app, cada comando é liberado só para ela, e a janela de login do Google não tem nenhuma permissão.
- **Arquivos auxiliares e log:** sem arquivos temporários em claro, sem core dumps, cofre fora do Time Machine e do Spotlight; o log de diagnóstico fica desligado no app final e nunca registra mensagens, telefones, senhas, chaves ou tokens.
- **Conta Google:** o acesso fica só na memória, vale 1 hora e é apagado no fim do download; o login acontece numa janela anônima.
- **Export HTML:** mensagens sempre cifradas, uma senha não abre as conversas de outra, nomes de arquivo aleatórios e a página não acessa a rede.

**Limitações:** malware rodando com o seu usuário enquanto o cofre está aberto consegue ler a memória ou a tela; uma senha fraca pode ser atacada offline por quem copiar o cofre; não existe recuperação de senha; o fluxo do Google não é oficial e pode parar de funcionar (a importação local continua).

Os detalhes técnicos e como verificar cada proteção estão nas seções em inglês acima.
