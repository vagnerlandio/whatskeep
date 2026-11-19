# Histórico WhatsApp

App desktop (Tauri v2 + React) que baixa o backup do WhatsApp/WhatsApp Business (Google Drive ou cópia local do celular), descriptografa **localmente** e mostra o histórico com visual do WhatsApp.

## Estrutura

```
apps/desktop/        App Tauri (React em src/, Rust em src-tauri/)
crates/wa-crypt/     Descriptografia crypt14/crypt15, sem I/O e sem dependência do Tauri
packages/            (futuro) viewer compartilhado e shell do export HTML cifrado
```

## Rodar

```sh
pnpm install
pnpm dev               # abre o app em modo desenvolvimento
cargo test --workspace # testes Rust
```

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
- Nunca versione backups ou chaves reais. O `.gitignore` bloqueia esses arquivos; só passam os fixtures sintéticos de `crates/wa-crypt/tests/fixtures/`.

## Fixtures de teste

Gerados com uma implementação independente ([wa-crypt-tools](https://github.com/ElDavoo/wa-crypt-tools) 0.1.0), usando chaves aleatórias e um SQLite fictício:

```sh
waencrypt --type 15 "$(cat key15.hex)" plain.db msgstore.db.crypt15
wacreatekey --hex "$(cat key15.hex)"          # → encrypted_backup.key
wacreatekey -c14 --hex <hex aleatório>        # → key
waencrypt --type 14 key plain.db msgstore.db.crypt14
```
