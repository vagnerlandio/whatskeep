# WhatsKeep

**Your WhatsApp conversations, safe and accessible.**

*Versão em português no [final deste arquivo](#português).*

Desktop app (Tauri v2 + React) that imports WhatsApp and WhatsApp Business backups — from Google Drive or from a copy taken from the phone — decrypts them **locally**, keeps them in an encrypted vault and shows the history with a WhatsApp-like interface. It can also produce an encrypted offline HTML export. The interface is available in English and Portuguese.

## What it can open

| Backup | What you need | Supported |
|---|---|---|
| End-to-end encrypted with a 64-digit key (`.crypt15`) | the 64 digits or the `encrypted_backup.key` file | ✅ |
| Not end-to-end encrypted (`.crypt14`) | the phone's `key` file (requires root) | ✅ |
| End-to-end encrypted with a password only | the key lives in WhatsApp's HSM | ❌ impossible outside WhatsApp |

**No root?** Turn on the end-to-end encrypted backup with a 64-digit key and make a new backup. In WhatsApp: *Settings → Chats → Chat backup → End-to-end encrypted backup → Turn on → Use 64-digit encryption key instead*. Write the key down somewhere safe; WhatsApp Business has its own key.

## Using it

1. **Create the vault** with a password (at least 8 characters; a long phrase is best). There is no password recovery.
2. **Import** the history:
   - **From Google Drive:** enter the account e-mail, sign in in the Google window that opens (the e-mail is already filled in), pick the backup and enter the key. Optionally download photos, videos and audio as well.
   - **From a file:** choose `msgstore.db.crypt15` (or `.crypt14`) copied from the phone (`Android/media/com.whatsapp/WhatsApp/Databases`) and enter the key.
3. **Media from the phone (optional):** import the `WhatsApp` folder copied from the phone; only files used by the imported messages are kept.
4. **Browse and search** your chats. The vault locks itself after 5 minutes without use.
5. **Export (optional):** create a folder that opens offline in any browser, protected by a password — all chats, some chats, or one password per chat.

Switch the language in the top bar or in the side bar (Português / English).

## Development

```sh
pnpm install
pnpm dev               # run the app in development mode
pnpm dev:log           # same, saving the diagnostics log to logs/
cargo test --workspace # Rust tests
pnpm test:e2e          # opens real exports in Chromium and WebKit (file://)
pnpm lint:security     # forbidden patterns in the frontend
```

Guide for AI agents and new contributors: [AGENTS.md](AGENTS.md), with [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) and [docs/RECIPES.md](docs/RECIPES.md). Threat model, protections and limitations: [SECURITY.md](SECURITY.md).

```
apps/desktop/           Tauri app (React in src/, Rust in src-tauri/)
crates/wa-crypt/        crypt14/crypt15 and .mcrypt1 decryption, no I/O, no Tauri
crates/wa-import/       msgstore (modern and legacy schemas) → the app's schema, with FTS5 search
crates/wa-vault/        Vault: SQLCipher per source app, Argon2id-protected key; viewer queries
crates/wa-gdrive/       Google Play Services sign-in and Google Drive backup download (unofficial)
crates/wa-export/       Encrypted HTML export (format v1)
packages/viewer/        Shared WhatsApp-style viewer (takes a DataSource)
packages/export-shell/  The export's index.html: decrypts in the browser (WebCrypto + Argon2id in WASM)
```

## Security rules

- All cryptography runs in Rust. Keys and plaintext stay in `Zeroizing` buffers and never reach the webview.
- No plaintext is ever written to disk: the history goes into an encrypted vault (SQLCipher + Argon2id).
- The Google token exists only in memory and is discarded after the download.
- Messages are untrusted content: no `dangerouslySetInnerHTML`, no link previews, strict CSP (see `tauri.conf.json`).
- Never commit real backups or keys. `.gitignore` blocks them; only the synthetic fixtures in `crates/*/tests/fixtures/` are allowed.

## Vault

Located at `~/Library/Application Support/com.local.wahistorico/vault/` (macOS; elsewhere, the app's data folder). The bundle identifier keeps its original name on purpose: changing it would move the data folder.

```
vault.json            Argon2id (256 MiB, t=3, p=4) + data key (DEK) wrapped with AES-256-GCM
stores/whatsapp.db    SQLCipher, key = HKDF(DEK, vault id, "wa-vault store v1:whatsapp")
stores/business.db    SQLCipher, key = HKDF(DEK, vault id, "wa-vault store v1:business")
stores/<app>-media.db media index (SQLCipher): sha256 → file, phone path → sha256
stores/<app>-media/   one encrypted file per media item (random name)
```

Media: AES-256-GCM in 1 MiB segments (STREAM scheme: nonce = prefix ‖ counter ‖ last flag; AAD = file id), so ranges can be read (video seeking) without decrypting everything. The index lives outside the message database, so re-importing the msgstore keeps the media. Media are linked to messages by WhatsApp's `file_hash` or by path (`Media/...`).

- The password only unwraps the DEK; changing it does not re-encrypt the databases. **There is no recovery**: without the password, you have to import again.
- The Argon2id parameters are part of the AAD: editing `vault.json` to weaken the KDF makes opening fail.
- Importing replaces that app's database atomically (new database in an encrypted temp file, then `rename`). A failure halfway keeps the previous one.
- The vault locks after 5 minutes without interaction (and with the "Lock" button); the DEK is zeroed in memory.
- Folders 0700, files 0600, `.metadata_never_index` for Spotlight, excluded from Time Machine.

## Google Drive (unofficial)

Same path as the WhatsApp app on Android, like [wabdd](https://github.com/giacomoferretti/whatsapp-backup-downloader-decryptor) and [whapa](https://github.com/B16f00t/whapa):

1. An **incognito** Tauri window with `accounts.google.com/EmbeddedSetup` (e-mail prefilled). No IPC permissions; it can only navigate Google domains.
2. The backend reads the `oauth_token` cookie and destroys the window.
3. Cookie → master token (`android.clients.google.com/auth`) → 1-hour access tokens for `com.whatsapp` and `com.whatsapp.w4b`. The master token is zeroed right after.
4. `backup.googleapis.com/v1/clients/wa/backups` → `Databases/msgstore.db.crypt15`, downloaded into memory with an MD5 check, decrypted and imported.

Access tokens are dropped when the download ends, on cancel, when the vault locks, or after 50 minutes. The HTTP client offers no ALPN and does not follow redirects (Google rejects the sign-in with ALPN; see gpsoauth).

Validated with a real account (2026-11):
- signing in through the embedded WebKit works (with Tauri's `freezePrototype` off and a Safari user agent in the login window);
- WhatsApp Business accepts the same signature (`client_sig`), and both apps' tokens see the same backup listing;
- current metadata reports encryption in `encryptedBackupMetadata` (the old `encryptedBackupEnabled` field is gone);
- Google issues the token with the scopes already granted to WhatsApp (`drive.appdata`, `drive.file`, `userinfo.email`, `userinfo.profile`, `openid`), not only `drive.appdata`.

Media (optional): in non-E2E backups they come in plain form (`Media/...`, MD5 checked). In E2E backups they come as `<hash>.mcrypt1`, with the real name in encrypted metadata (HMAC + AES-CBC) and the content in AES-GCM with a per-file key (`wa_crypt::mcrypt1`). Whether the file carries the GCM tag is undocumented: the app tries with the tag and, if it does not verify, only accepts the content when its SHA-256 matches the msgstore's `file_hash`. Not yet validated with a real backup.

## HTML export

A folder that opens offline in any browser:

```
index.html            viewer (a single file), CSP without network
data/manifest.js      Argon2id (64 MiB, t=3) + one encrypted slot per password
data/<id>.js          encrypted chunks (AES-GCM, per-chat key via HKDF), size rounded to 64 KiB
media/pack-NNN.bin    encrypted media (1 MiB STREAM, per-media key) — or media/<id> in plain form, if chosen
```

- All chats, some chats, or one password per chat. A password neither opens nor lists another password's chats, and search only covers unlocked chats. Chats left out are not included at all.
- Random file names; no phone number, text or path in plain form (tested).
- On `file://` browsers block `fetch`, so data is loaded as `<script src>`. Encrypted media: the user selects the folder once (`<input webkitdirectory>`) and files are read with `File.slice`.
- CSP: `connect-src 'none'`, scripts only from the package (inline by hash). **No `'self'`**: in WebKit (Safari), `'self'` on a `file://` page allows any remote URL; `file:` does not.
- Locks itself after 15 minutes without use (reloads the page and forgets the keys).

## Test fixtures

Generated with an independent implementation ([wa-crypt-tools](https://github.com/ElDavoo/wa-crypt-tools) 0.1.0), using random keys and a fictitious SQLite database:

```sh
waencrypt --type 15 "$(cat key15.hex)" plain.db msgstore.db.crypt15
wacreatekey --hex "$(cat key15.hex)"          # → encrypted_backup.key
wacreatekey -c14 --hex <random hex>           # → key
waencrypt --type 14 key plain.db msgstore.db.crypt14
```

---

## Português

**Suas conversas do WhatsApp, seguras e acessíveis.**

App desktop (Tauri v2 + React) que importa backups do WhatsApp e do WhatsApp Business — do Google Drive ou de uma cópia tirada do celular —, descriptografa **localmente**, guarda num cofre cifrado e mostra o histórico com visual do WhatsApp. Também gera um export HTML offline cifrado. A interface está em português e inglês.

### O que dá para abrir

| Backup | O que é preciso | Suportado |
|---|---|---|
| Criptografia de ponta a ponta com chave de 64 dígitos (`.crypt15`) | os 64 dígitos ou o arquivo `encrypted_backup.key` | ✅ |
| Sem criptografia de ponta a ponta (`.crypt14`) | o arquivo `key` do celular (exige root) | ✅ |
| Criptografia de ponta a ponta só com senha | a chave fica no HSM do WhatsApp | ❌ impossível fora do WhatsApp |

**Sem root?** Ative o backup com criptografia de ponta a ponta usando a chave de 64 dígitos e faça um backup novo. No WhatsApp: *Configurações → Conversas → Backup de conversas → Backup criptografado de ponta a ponta → Ativar → Usar chave de criptografia de 64 dígitos*. Anote a chave num lugar seguro; o WhatsApp Business tem uma chave própria.

### Como usar

1. **Crie o cofre** com uma senha (mínimo de 8 caracteres; uma frase longa é o ideal). Não existe recuperação de senha.
2. **Importe** o histórico:
   - **Pelo Google Drive:** digite o e-mail da conta, entre na janela do Google que abrir (o e-mail já vem preenchido), escolha o backup e informe a chave. Se quiser, baixe também fotos, vídeos e áudios.
   - **Por arquivo:** escolha o `msgstore.db.crypt15` (ou `.crypt14`) copiado do celular (`Android/media/com.whatsapp/WhatsApp/Databases`) e informe a chave.
3. **Mídias do celular (opcional):** importe a pasta `WhatsApp` copiada do celular; só entram os arquivos usados pelas mensagens importadas.
4. **Navegue e pesquise** nas conversas. O cofre trava sozinho após 5 minutos sem uso.
5. **Exporte (opcional):** gere uma pasta que abre offline em qualquer navegador, protegida por senha — todas as conversas, só algumas ou uma senha por conversa.

O idioma se troca na barra superior ou na barra lateral (Português / English).

### Desenvolvimento

```sh
pnpm install
pnpm dev               # abre o app em modo desenvolvimento
pnpm dev:log           # idem, gravando o log de diagnóstico em logs/
cargo test --workspace # testes Rust
pnpm test:e2e          # abre exports reais no Chromium e no WebKit (file://)
pnpm lint:security     # padrões proibidos no frontend
```

### Segurança, em resumo

- Toda a criptografia roda em Rust; chaves e texto puro nunca chegam à interface.
- Nada em claro é gravado em disco: o histórico fica num cofre cifrado (SQLCipher + Argon2id).
- O acesso ao Google existe só na memória e é descartado depois do download.
- Mensagens são tratadas como conteúdo não confiável: sem HTML, sem pré-visualização de links, links só abrem após confirmação.
- Nunca versione backups ou chaves reais.

Os detalhes técnicos (formato do cofre, fluxo do Google Drive, formato do export, fixtures de teste) estão nas seções em inglês acima e no [SECURITY.md](SECURITY.md).
