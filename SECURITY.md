# Segurança

O app lida com o histórico inteiro de conversas de alguém. Este documento diz
contra o que ele protege, como, o que fica de fora e como verificar.

## Contra o que protege

| Ameaça | Proteção |
|---|---|
| Alguém com acesso ao disco (computador roubado, backup do disco, outra conta) | Tudo o que é importado fica cifrado no cofre (SQLCipher + AES-256-GCM), com chave derivada da senha por Argon2id. Nada em claro é gravado. |
| Alguém diante do computador destravado | Trava após 5 min sem uso e no botão "Travar"; a chave é zerada da memória. Trocar a senha pede a senha atual. Senhas erradas seguidas atrasam as próximas tentativas (até 30 s). |
| Mensagem maliciosa (HTML, script, link enganoso) | Texto de mensagem é sempre renderizado como texto. Links só abrem depois de confirmação, só http/https, sem `usuário@host`. CSP sem rede. |
| Página remota ganhando acesso ao app | A janela principal só navega dentro do app; os comandos do app são liberados um a um, só para ela (ACL testada em execução). A janela de login do Google não tem nenhuma permissão. |
| Vazamento por arquivos auxiliares | `temp_store = MEMORY` em todas as conexões (inclusive no msgstore decifrado), sem core dumps, cofre fora do Time Machine e do Spotlight, mídias salvas só por ação explícita. |
| Roubo da conta Google | O token mestre do Google existe por instantes e é zerado; só tokens de 1 h com escopo `drive.appdata` ficam em memória, até o fim do download. Nada é gravado. Login numa janela anônima. |
| Export HTML parar em mãos erradas | Mensagens sempre cifradas (Argon2id + AES-GCM, chave por conversa). Uma senha não abre nem lista as conversas de outra. Nomes de arquivo aleatórios, sem telefone ou texto em claro. A página não acessa a rede. |

## Detalhes

### Cofre (`crates/wa-vault`)
- `vault.json`: Argon2id (256 MiB, t=3, p=4) → chave que desembrulha a DEK (AES-256-GCM). Os parâmetros do KDF entram no AAD: baixá-los editando o arquivo faz a abertura falhar.
- Um banco SQLCipher por app de origem, chave = HKDF(DEK, id do cofre, rótulo). Chave bruta (sem o PBKDF2 do SQLCipher), `cipher_memory_security = ON`.
- Mídias: um arquivo por mídia, segmentos AES-GCM de 1 MiB (STREAM: nonce com contador e marca de último; AAD = id do arquivo). Detecta alteração, troca e truncamento.
- Pastas 0700, arquivos 0600, `.metadata_never_index`, exclusão do Time Machine (`tmutil isexcluded` confirma).

### App (`apps/desktop`)
- Criptografia toda em Rust; o webview recebe só páginas de mensagens e URLs `wamedia://` (mídia servida do cofre com `Cache-Control: no-store` e lista fechada de tipos MIME).
- CSP de produção: `default-src 'self'`, `connect-src ipc:`, imagens/mídia só do app e de `wamedia:`, sem `data:`/`blob:`.
- Capabilities: só `event:listen/unlisten`, `dialog:open` e os comandos do app (manifesto em `build.rs`). Sem `core:webview:default` (que inclui abrir o devtools) e sem `core:window:default`.
- Devtools não entram no build de release (feature `devtools` desligada).
- Caminhos de arquivo para gravar (export, salvar mídia) são escolhidos no seletor nativo pelo backend; o webview nunca informa onde gravar.
- `panic = "abort"`, binário sem símbolos.

### Export HTML (`crates/wa-export`, `packages/export-shell`)
- Formato em `crates/wa-export/src/format.rs`.
- CSP injetada com o hash dos scripts inline e `connect-src 'none'`. **Sem `'self'`**: no WebKit, `'self'` numa página `file://` libera qualquer URL remota (verificado: buscou imagem e script de um servidor externo). Com `file:`, nenhum dos dois motores acessa a rede.
- Trava (recarrega e esquece as chaves) após 15 min sem uso.

## Limitações conhecidas

- **Malware rodando como o seu usuário** com o cofre destravado consegue ler a memória do app ou capturar a tela. Nenhum app desktop comum resolve isso.
- **Senha fraca**: quem copiar o `vault.json` (ou um export) pode tentar senhas offline. O Argon2id encarece cada tentativa, mas uma senha longa é o que realmente protege. Não há recuperação de senha.
- **Senhas e a chave de 64 dígitos passam pelo webview** ao serem digitadas (inevitável numa UI web); no Rust ficam em buffers zerados.
- **Memória do sistema**: o SQLCipher e o Rust zeram o que liberam, mas o sistema pode ter paginado memória para o swap (no macOS o swap é cifrado).
- **SSD/APFS**: arquivos apagados não são sobrescritos de verdade; por isso nada em claro é gravado em primeiro lugar.
- **Fluxo do Google não oficial**: pode quebrar ou ser bloqueado a qualquer momento; a importação local continua funcionando.
- **Windows/Linux**: a exclusão de backup do sistema só existe no macOS.
- `cargo audit`: nenhuma vulnerabilidade; 6 avisos de crates sem manutenção/unsound na camada Linux do Tauri (GTK/glib, D-Bus), sem correção disponível e fora do build de macOS.

## Como verificar

```sh
cargo test --workspace          # cripto, cofre, isolamento, adulteração, ACL, vazamentos
pnpm test:e2e                   # export aberto por file:// no Chromium e no WebKit, incluindo sonda de rede
pnpm lint:security              # padrões proibidos no frontend (innerHTML, eval, href dinâmico...)
cargo audit && pnpm audit --prod
```

Testes que valem destacar:
- `wa-vault/tests/vault.rs`: nenhum texto, telefone, schema ou senha em claro nos arquivos do cofre; exclusão do Time Machine.
- `wa-vault/tests/tempfiles.rs` e `wa-import/tests/import.rs`: `temp_store = MEMORY` em todas as conexões com dados decifrados.
- `wa-export/tests/export.rs`: isolamento entre senhas, nada em claro no pacote, adulteração detectada, CSP sem `'self'`.
- `apps/desktop/src-tauri/src/lib.rs` (`acl_so_a_janela_principal_chama_comandos`): ACL real em execução.
