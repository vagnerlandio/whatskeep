# Histórico WhatsApp

App desktop (Tauri v2 + React) que baixa o backup do WhatsApp/WhatsApp Business (Google Drive ou cópia local do celular), descriptografa **localmente** e mostra o histórico com visual do WhatsApp.

## Estrutura

Guia para agentes de IA e novos colaboradores: [AGENTS.md](AGENTS.md) (com [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) e [docs/RECIPES.md](docs/RECIPES.md)).

```
apps/desktop/        App Tauri (React em src/, Rust em src-tauri/)
crates/wa-crypt/     Descriptografia crypt14/crypt15, sem I/O e sem dependência do Tauri
crates/wa-import/    Importa o msgstore (schemas novo e legado) para o schema do app, com busca FTS5
crates/wa-vault/     Cofre: SQLCipher por app de origem, chave protegida por Argon2id; consultas do visualizador
crates/wa-gdrive/    Login do Google Play Services e download do backup do Google Drive (não oficial)
crates/wa-export/    Gera o export HTML cifrado (formato v1)
packages/viewer/     Visualizador estilo WhatsApp compartilhado (recebe um DataSource)
packages/export-shell/  index.html do export: decifra no navegador (WebCrypto + Argon2id em WASM)
```

## Rodar

```sh
pnpm install
pnpm dev               # abre o app em modo desenvolvimento
cargo test --workspace # testes Rust
pnpm test:e2e          # abre exports reais no Chromium e no WebKit (file://)
pnpm lint:security     # padrões proibidos no frontend
```

Modelo de ameaças, proteções e limitações: [SECURITY.md](SECURITY.md).

## Que backups dá para abrir

| Backup | O que é preciso | Suportado |
|---|---|---|
| E2E com chave de 64 dígitos (`.crypt15`) | os 64 dígitos ou o `encrypted_backup.key` | ✅ |
| Sem E2E (`.crypt14`) | arquivo `key` do celular (exige root) | ✅ |
| E2E protegido só por senha | a chave fica no HSM do WhatsApp | ❌ impossível fora do app |

## Regras de segurança

- Toda a criptografia roda em Rust. Chaves e texto puro ficam em buffers `Zeroizing` e nunca chegam ao webview.
- Nenhum texto puro é gravado em disco de forma persistente: o histórico vai para um cofre cifrado (SQLCipher + Argon2id).
- O token Google existe só em memória e é descartado depois do download.
- Mensagens são conteúdo não confiável: proibido `dangerouslySetInnerHTML`, sem pré-visualização de links, CSP estrita (veja `tauri.conf.json`).
- Nunca versione backups ou chaves reais. O `.gitignore` bloqueia esses arquivos; só passam os fixtures sintéticos de `crates/*/tests/fixtures/`.

## Cofre

Fica em `~/Library/Application Support/com.local.wahistorico/vault/` (macOS; nos outros sistemas, a pasta de dados do app):

```
vault.json            Argon2id (256 MiB, t=3, p=4) + chave de dados (DEK) embrulhada com AES-256-GCM
stores/whatsapp.db    SQLCipher, chave = HKDF(DEK, id do cofre, "wa-vault store v1:whatsapp")
stores/business.db    SQLCipher, chave = HKDF(DEK, id do cofre, "wa-vault store v1:business")
stores/<app>-media.db índice das mídias (SQLCipher): sha256 → arquivo, caminho no celular → sha256
stores/<app>-media/   um arquivo cifrado por mídia (nome aleatório)
```

Mídias: AES-256-GCM em segmentos de 1 MiB (esquema STREAM: nonce = prefixo ‖ contador ‖ último; AAD = id do arquivo), o que permite ler trechos (seek de vídeo) sem decifrar tudo. O índice fica fora do banco de mensagens, então reimportar o msgstore não perde as mídias. A ligação com as mensagens é pelo `file_hash` do WhatsApp ou pelo caminho (`Media/...`).

- A senha só desembrulha a DEK. Trocar a senha não recifra os bancos. **Não há recuperação**: sem a senha, é preciso importar de novo.
- Os parâmetros do Argon2id entram no AAD: editar o `vault.json` para baratear o KDF faz a abertura falhar.
- Importar substitui o banco daquele app de forma atômica (banco novo num temporário cifrado, depois `rename`). Uma falha no meio mantém o anterior.
- O cofre trava após 5 minutos sem interação (e no botão "Travar"); a DEK é zerada da memória.
- Pastas 0700, arquivos 0600, `.metadata_never_index` para o Spotlight.

## Google Drive (não oficial)

Mesmo caminho do app do WhatsApp no Android, como o [wabdd](https://github.com/giacomoferretti/whatsapp-backup-downloader-decryptor) e o [whapa](https://github.com/B16f00t/whapa):

1. Janela **anônima** do Tauri com `accounts.google.com/EmbeddedSetup`. Sem permissões de IPC; só navega por domínios do Google.
2. O backend lê o cookie `oauth_token` e destrói a janela.
3. Cookie → token mestre (`android.clients.google.com/auth`) → tokens `drive.appdata` de 1 h para `com.whatsapp` e `com.whatsapp.w4b`. O token mestre é zerado logo em seguida.
4. `backup.googleapis.com/v1/clients/wa/backups` → `Databases/msgstore.db.crypt15`, baixado para a memória com conferência de MD5, decifrado e importado.

Os tokens de acesso somem ao fim do download, ao cancelar, ao travar o cofre ou após 50 minutos. O cliente HTTP não oferece ALPN e não segue redirecionamentos (o Google recusa o login com ALPN; ver gpsoauth).

Ainda não validado com uma conta real:
- se o Google aceita o login pelo WebKit embutido (senão, há o login manual colando o cookie);
- se o WhatsApp Business usa a mesma assinatura (`client_sig`) e se os backups dele aparecem na mesma listagem.

Mídias (opcional): em backups sem E2E vêm em claro (`Media/...`, MD5 conferido). Em backups E2E vêm como `<hash>.mcrypt1`, com o nome real em metadados cifrados (HMAC + AES-CBC) e o conteúdo em AES-GCM com chave por arquivo (`wa_crypt::mcrypt1`). Não está documentado se o arquivo traz a tag GCM: o app tenta com a tag e, se não conferir, só aceita o conteúdo se o SHA-256 bater com o `file_hash` do msgstore. Formato ainda não validado com um backup real.

## Export HTML

Pasta que abre offline em qualquer navegador:

```
index.html            visualizador (um arquivo só), CSP sem rede
data/manifest.js      Argon2id (64 MiB, t=3) + um slot cifrado por senha
data/<id>.js          pedaços cifrados (AES-GCM, chave por conversa via HKDF), tamanho arredondado a 64 KiB
media/pack-NNN.bin    mídias cifradas (STREAM 1 MiB, chave por mídia) — ou media/<id> em claro, se escolhido
```

- Todas as conversas, só algumas, ou uma senha por conversa. Uma senha não abre nem lista as conversas de outra, e a busca só cobre as destravadas. Conversas fora do export não entram em nada.
- Nomes de arquivo aleatórios; nenhum telefone, texto ou caminho em claro (testado).
- Em `file://` o navegador bloqueia `fetch`, então os dados são carregados como `<script src>`. Mídias cifradas: o usuário seleciona a pasta uma vez (`<input webkitdirectory>`) e os arquivos são lidos com `File.slice`.
- CSP: `connect-src 'none'`, scripts só do pacote (inline por hash). **Sem `'self'`**: no WebKit (Safari), `'self'` numa página `file://` libera qualquer URL remota; `file:` não.
- Trava sozinho após 15 minutos sem uso (recarrega a página e esquece as chaves).

## Fixtures de teste

Gerados com uma implementação independente ([wa-crypt-tools](https://github.com/ElDavoo/wa-crypt-tools) 0.1.0), usando chaves aleatórias e um SQLite fictício:

```sh
waencrypt --type 15 "$(cat key15.hex)" plain.db msgstore.db.crypt15
wacreatekey --hex "$(cat key15.hex)"          # → encrypted_backup.key
wacreatekey -c14 --hex <hex aleatório>        # → key
waencrypt --type 14 key plain.db msgstore.db.crypt14
```
