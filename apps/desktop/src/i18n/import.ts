// Textos das telas de importação (arquivo local, Google Drive e pasta de mídias).
import type { ImportPhase } from "../api";
import { makeT } from "./errors";

const pt = {
  // Chave do backup
  keyLegend: "Chave do backup",
  keyTabHex: "64 dígitos",
  keyTabFile: "Arquivo encrypted_backup.key",
  crypt14KeyBefore: "Backups crypt14 não têm criptografia de ponta a ponta: a chave é o arquivo ",
  crypt14KeyAfter: " do celular (/data/data/com.whatsapp/files/key, só com root).",
  keyPlaceholder: "Cole aqui a chave de 64 dígitos mostrada quando você ativou o backup criptografado",
  keyBadChars: "A chave só tem os dígitos 0–9 e as letras a–f.",
  keyDigits: (n: number) => `${n}/64 dígitos`,
  noFileChosen: "Nenhum arquivo escolhido",
  chooseKeyFile: "Escolher arquivo de chave…",
  passwordOnly:
    "Se o backup foi protegido só com uma senha (sem a chave de 64 dígitos), não há como abri-lo fora do WhatsApp: a chave fica guardada nos servidores dele. No celular, troque a proteção para a chave de 64 dígitos e faça um backup novo.",

  // Origem
  importAs: "Importar como",

  // Fases da importação
  phase: {
    reading: "Lendo o arquivo…",
    downloading: "Baixando do Google Drive…",
    decrypting: "Descriptografando…",
    importing: "Importando mensagens e montando o índice de busca…",
    done: "Concluído",
  } satisfies Record<ImportPhase, string>,

  // Erros mais comuns
  friendly: {
    wrong_key: "A chave não abre este backup. Confira os 64 dígitos ou o arquivo de chave.",
    key_mismatch: "Este arquivo de chave é de outro formato de backup.",
    not_msgstore: "O arquivo foi descriptografado, mas não é um banco de mensagens (msgstore).",
  } as Record<string, string>,

  // Resumo
  importDone: "Importação concluída",
  summaryMain: (chats: string, messages: string, media: string) =>
    `${chats} conversas · ${messages} mensagens · ${media} mídias`,
  summaryExtra: (reactions: string, vcards: string, skipped: string) =>
    `${reactions} reações · ${vcards} contatos compartilhados · ${skipped} status ignorados`,
  unknownTypes: (list: string) =>
    `Tipos de mensagem ainda não reconhecidos: ${list}. Elas aparecem como "mensagem não suportada".`,
  importAnother: "Importar outro backup",

  // Mídias
  media: "Mídias",
  mediaAdded: (added: string, already: string) => `${added} novas · ${already} já estavam no cofre`,
  mediaFailed: (n: string) => ` · ${n} com erro`,
  mediaMatched: (matched: string, expected: string) =>
    `${matched} de ${expected} mídias do histórico encontradas na origem`,
  mediaUnmatched: (n: string) => ` · ${n} arquivos sem mensagem correspondente ignorados`,
  stoppedEarly: (reason: string) =>
    `Parou antes do fim: ${reason}. O que já entrou continua no cofre; rode de novo para continuar.`,
  stoppedCancelled: "cancelado",
  stoppedExpired: "a sessão do Google expirou; entre de novo para continuar",
  mediaProgress: (done: string, total: string, bytes: string, totalBytes: string, added: string) =>
    `${done} de ${total} arquivos · ${bytes} de ${totalBytes} · ${added} novos`,
  searchingFiles: "Procurando os arquivos…",
  stop: "Parar",

  // Importação de arquivo local
  importing: "Importando",
  inMemoryNote: "O conteúdo descriptografado fica só na memória e vai direto para o cofre.",
  importFromFile: "Importar de um arquivo",
  dropTitle: "Arraste o msgstore.db.crypt15 ou .crypt14 para cá",
  dropSubtitle: "No celular fica em Android/media/com.whatsapp/WhatsApp/Databases",
  chooseFile: "Escolher arquivo…",
  backupFilter: "Backup do WhatsApp",
  change: "Trocar",
  import: "Importar",
  replaceWarning: (app: string, at: string) =>
    `Já existe um histórico do ${app} importado em ${at}. Ele será substituído por este backup.`,

  // Pasta de mídias
  mediaFolderTitle: "Escolha a pasta WhatsApp (ou WhatsApp Business) copiada do celular",
  mediaFolderAgain: "Importar mídias de outra pasta…",
  mediaFolder: "Importar mídias de uma pasta…",

  // Google Drive
  drive: "Google Drive",
  driveFooter: "A sessão do Google foi encerrada e os tokens de acesso foram apagados da memória.",
  downloadingMedia: "Baixando fotos, vídeos e áudios…",
  ofTotal: (total: string) => ` de ${total}`,
  cancel: "Cancelar",
  driveInMemory: "O backup vem cifrado do Google e é aberto só na memória, direto para o cofre.",
  waitingLogin: "Entre na sua conta na janela do Google que foi aberta.",
  waitingHint:
    "Ela fecha sozinha quando o login terminar. Se o Google disser que o navegador não é seguro, cancele e use a opção manual.",
  manualTitle: "Google Drive: login manual",
  manualStep1Before: "Num navegador, abra ",
  manualStep1After: (account: string) => ` e entre na conta ${account}.`,
  manualAccountFallback: "do backup",
  manualStep2Before:
    "Quando a página parar de carregar, abra as ferramentas do desenvolvedor → Aplicativo/Armazenamento → Cookies → ",
  manualStep2After: ".",
  manualStep3Before: "Copie o valor do cookie ",
  manualStep3Middle: " (começa com ",
  manualStep3After: ") e cole abaixo. Ele vale por poucos minutos e só uma vez.",
  back: "Voltar",
  continue: "Continuar",
  driveBackups: "Backups no Google Drive",
  signOut: "Sair da conta",
  noBackups: "Nenhum backup do WhatsApp encontrado nesta conta.",
  backup: "Backup",
  endToEnd: "ponta a ponta",
  stillUploading: "Ainda sendo enviado pelo celular",
  chatDbSize: (size: string) => `mensagens: ${size}`,
  messagesCount: (n: string) => `${n} mensagens`,
  totalSize: (size: string) => `backup total: ${size}`,
  and: " e ",
  refused: (apps: string) =>
    `O Google não liberou acesso pelo ${apps}. Se você usa esse app, os backups dele podem não aparecer aqui.`,
  noE2eTitle: "Este backup não tem criptografia de ponta a ponta.",
  noE2eBefore: " A chave dele fica num arquivo interno do celular (",
  noE2eAfter: "), que só dá para copiar com root. Sem root, o caminho é:",
  noE2eStep1Before: "No WhatsApp: ",
  noE2eStep1Path:
    "Configurações → Conversas → Backup de conversas → Backup criptografado de ponta a ponta → Ativar",
  noE2eStep1After: ".",
  noE2eStep2Before: "Escolha ",
  noE2eStep2Option: "Usar chave de criptografia de 64 dígitos",
  noE2eStep2After: " (não a senha) e anote a chave num lugar seguro.",
  noE2eStep3: "Faça um novo backup e espere terminar de enviar.",
  noE2eStep4: "Volte aqui, entre de novo e use a chave de 64 dígitos.",
  noE2eBusiness: "No WhatsApp Business o caminho é o mesmo, com uma chave própria.",
  includeMedia: "Baixar também fotos, vídeos, áudios e documentos",
  upTo: (size: string) => ` (até ${size})`,
  includeMediaHint:
    "Pode demorar bastante. Cada arquivo entra cifrado no cofre; se parar no meio, rode de novo que ele continua de onde estava. O acesso ao Google vale 1 hora.",
  downloadAndImport: "Baixar e importar",
  driveDownload: "Baixar do Google Drive",
  introUnofficialBefore: "Usa o mesmo acesso do app do WhatsApp no Android. ",
  introUnofficial: "Não é um método oficial",
  introUnofficialAfter:
    ': o Google pode bloqueá-lo, e você pode receber um alerta de "novo dispositivo" na conta.',
  introMemory:
    "O acesso fica só na memória deste app, limitado à pasta de backup do WhatsApp, e é apagado ao terminar o download, ao travar o cofre ou depois de 50 minutos.",
  introPassword: "O login acontece numa janela do próprio Google; este app não vê a sua senha.",
  emailLabel: "E-mail da conta Google do backup",
  emailHint: "A janela do Google já abre com este e-mail preenchido.",
  signIn: "Entrar no Google",
  refusedWindow: "O Google recusou a janela de login?",
  manualLogin: "Entrar pelo navegador (login manual)",
};

const en: typeof pt = {
  keyLegend: "Backup key",
  keyTabHex: "64 digits",
  keyTabFile: "encrypted_backup.key file",
  crypt14KeyBefore: "crypt14 backups have no end-to-end encryption: the key is the phone's ",
  crypt14KeyAfter: " file (/data/data/com.whatsapp/files/key, root only).",
  keyPlaceholder: "Paste the 64-digit key shown when you turned on the encrypted backup",
  keyBadChars: "The key only has the digits 0–9 and the letters a–f.",
  keyDigits: (n) => `${n}/64 digits`,
  noFileChosen: "No file chosen",
  chooseKeyFile: "Choose key file…",
  passwordOnly:
    "If the backup was protected with a password only (no 64-digit key), it can't be opened outside WhatsApp: the key is kept on WhatsApp's servers. On the phone, switch the protection to the 64-digit key and make a new backup.",

  importAs: "Import as",

  phase: {
    reading: "Reading the file…",
    downloading: "Downloading from Google Drive…",
    decrypting: "Decrypting…",
    importing: "Importing messages and building the search index…",
    done: "Done",
  },

  friendly: {
    wrong_key: "The key does not open this backup. Check the 64 digits or the key file.",
    key_mismatch: "This key file is for a different backup format.",
    not_msgstore: "The file was decrypted, but it is not a message database (msgstore).",
  },

  importDone: "Import complete",
  summaryMain: (chats, messages, media) => `${chats} chats · ${messages} messages · ${media} media files`,
  summaryExtra: (reactions, vcards, skipped) =>
    `${reactions} reactions · ${vcards} shared contacts · ${skipped} status updates skipped`,
  unknownTypes: (list) => `Message types not recognized yet: ${list}. They show up as "unsupported message".`,
  importAnother: "Import another backup",

  media: "Media",
  mediaAdded: (added, already) => `${added} new · ${already} already in the vault`,
  mediaFailed: (n) => ` · ${n} failed`,
  mediaMatched: (matched, expected) => `${matched} of ${expected} media files from the history found in the source`,
  mediaUnmatched: (n) => ` · ${n} files with no matching message skipped`,
  stoppedEarly: (reason) =>
    `Stopped before the end: ${reason}. What was already imported stays in the vault; run it again to continue.`,
  stoppedCancelled: "cancelled",
  stoppedExpired: "the Google session expired; sign in again to continue",
  mediaProgress: (done, total, bytes, totalBytes, added) =>
    `${done} of ${total} files · ${bytes} of ${totalBytes} · ${added} new`,
  searchingFiles: "Looking for the files…",
  stop: "Stop",

  importing: "Importing",
  inMemoryNote: "The decrypted content stays in memory only and goes straight into the vault.",
  importFromFile: "Import from a file",
  dropTitle: "Drag msgstore.db.crypt15 or .crypt14 here",
  dropSubtitle: "On the phone it is in Android/media/com.whatsapp/WhatsApp/Databases",
  chooseFile: "Choose file…",
  backupFilter: "WhatsApp backup",
  change: "Change",
  import: "Import",
  replaceWarning: (app, at) => `A ${app} history imported on ${at} already exists. It will be replaced by this backup.`,

  mediaFolderTitle: "Choose the WhatsApp (or WhatsApp Business) folder copied from the phone",
  mediaFolderAgain: "Import media from another folder…",
  mediaFolder: "Import media from a folder…",

  drive: "Google Drive",
  driveFooter: "The Google session was closed and the access tokens were erased from memory.",
  downloadingMedia: "Downloading photos, videos and audio…",
  ofTotal: (total) => ` of ${total}`,
  cancel: "Cancel",
  driveInMemory: "The backup comes encrypted from Google and is opened in memory only, straight into the vault.",
  waitingLogin: "Sign in to your account in the Google window that opened.",
  waitingHint:
    "It closes by itself when sign-in is done. If Google says the browser is not secure, cancel and use the manual option.",
  manualTitle: "Google Drive: manual sign-in",
  manualStep1Before: "In a browser, open ",
  manualStep1After: (account) => ` and sign in to ${account}.`,
  manualAccountFallback: "the backup account",
  manualStep2Before: "When the page stops loading, open the developer tools → Application/Storage → Cookies → ",
  manualStep2After: ".",
  manualStep3Before: "Copy the value of the ",
  manualStep3Middle: " cookie (it starts with ",
  manualStep3After: ") and paste it below. It is valid for a few minutes and only once.",
  back: "Back",
  continue: "Continue",
  driveBackups: "Backups on Google Drive",
  signOut: "Sign out",
  noBackups: "No WhatsApp backup found in this account.",
  backup: "Backup",
  endToEnd: "end-to-end",
  stillUploading: "Still being uploaded by the phone",
  chatDbSize: (size) => `messages: ${size}`,
  messagesCount: (n) => `${n} messages`,
  totalSize: (size) => `total backup: ${size}`,
  and: " and ",
  refused: (apps) =>
    `Google did not grant access through ${apps}. If you use that app, its backups may not show up here.`,
  noE2eTitle: "This backup has no end-to-end encryption.",
  noE2eBefore: " Its key is in an internal file on the phone (",
  noE2eAfter: "), which can only be copied with root. Without root, the steps are:",
  noE2eStep1Before: "In WhatsApp: ",
  noE2eStep1Path: "Settings → Chats → Chat backup → End-to-end encrypted backup → Turn on",
  noE2eStep1After: ".",
  noE2eStep2Before: "Choose ",
  noE2eStep2Option: "Use 64-digit encryption key instead",
  noE2eStep2After: " (not the password) and write the key down somewhere safe.",
  noE2eStep3: "Make a new backup and wait for the upload to finish.",
  noE2eStep4: "Come back here, sign in again and use the 64-digit key.",
  noE2eBusiness: "In WhatsApp Business the steps are the same, with its own key.",
  includeMedia: "Also download photos, videos, audio and documents",
  upTo: (size) => ` (up to ${size})`,
  includeMediaHint:
    "This can take a long time. Each file goes into the vault encrypted; if it stops midway, run it again and it picks up where it left off. Google access lasts 1 hour.",
  downloadAndImport: "Download and import",
  driveDownload: "Download from Google Drive",
  introUnofficialBefore: "Uses the same access as the WhatsApp app on Android. ",
  introUnofficial: "This is not an official method",
  introUnofficialAfter: ': Google may block it, and you may get a "new device" alert on the account.',
  introMemory:
    "The access stays only in this app's memory, limited to the WhatsApp backup folder, and is erased when the download ends, when the vault is locked or after 50 minutes.",
  introPassword: "Sign-in happens in Google's own window; this app never sees your password.",
  emailLabel: "Email of the Google account with the backup",
  emailHint: "The Google window opens with this email already filled in.",
  signIn: "Sign in to Google",
  refusedWindow: "Did Google refuse the sign-in window?",
  manualLogin: "Sign in through the browser (manual sign-in)",
};

export const useImportT = makeT({ "pt-BR": pt, en });
