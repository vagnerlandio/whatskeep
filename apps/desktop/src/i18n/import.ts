// Texts for the import screens (local file, Google Drive and media folder).
import type { DriveMode, ImportPhase } from "../api";
import type { JobStep } from "../jobs";
import { makeT } from "./errors";

const pt = {
  // Redesigned screens
  listTitle: "Escolha o backup",
  listSubtitle: (n: number) => (n === 1 ? "1 backup encontrado nesta conta Google" : `${n} backups encontrados nesta conta Google`),
  sectionBackup: "Backup",
  noE2eTag: "sem criptografia ponta a ponta",
  importAsHint: "O Google não informa de qual app é cada backup. Confira antes de importar.",
  mediaSwitch: "Fotos, vídeos, áudios e documentos",
  mediaSwitchSize: (size: string) => `Até ${size}`,
  mediaSwitchDetail: "Incluídos no tamanho total do backup",
  driveIntroSubtitle: "Entre na conta Google onde fica o backup do WhatsApp.",
  howItWorks: "Como funciona e o que o app acessa",
  fileSubtitle: "Um backup baixado antes ou copiado do celular.",
  whereIsTheBackup: "Onde encontrar o backup",
  whereDrive:
    "Baixado do Google Drive pelo WhatsKeep: na pasta que você escolheu, em whatsapp-backup-AAAA-MM-DD/Databases.",
  wherePhone:
    "No celular Android: Android/media/com.whatsapp/WhatsApp/Databases. Copie a pasta WhatsApp inteira para trazer também as fotos e vídeos.",
  sectionFile: "Arquivo",
  withFolderMediaDetail: "Da pasta Media ao lado do arquivo",
  passwordOnlySummary: "Meu backup usa senha, não chave",
  jobTitleDrive: "Baixando do Google Drive",
  jobTitleLocal: "Importando o backup",
  jobTitleMedia: "Importando mídias",
  stepLabel: {
    download: "Baixar o banco de mensagens",
    import: "Descriptografar e importar as mensagens",
    files: "Baixar fotos, vídeos e áudios para a pasta",
    media: "Guardar fotos, vídeos e áudios no cofre",
    mediaDownload: "Baixar fotos, vídeos e áudios para o cofre",
  } satisfies Record<JobStep, string>,
  donePartial: "Concluído, com pendências",
  statChats: "conversas",
  statMessages: "mensagens",
  statMedia: "mídias no histórico",
  statMediaInVault: "mídias no cofre",
  statFiles: "arquivos na pasta",
  statSaved: "salvos",
  alsoImported: "Também importados",
  unknownTypesSummary: (types: number, messages: string) =>
    `${messages} mensagens de ${types} tipos ainda não reconhecidos`,

  // Start: where the history comes from
  hubTitle: "Como você quer trazer o seu histórico?",
  hubNoVault:
    "O histórico fica guardado num cofre cifrado neste computador. A senha do cofre é criada quando você importar.",
  hubDriveTitle: "Baixar do Google Drive",
  hubDriveText: "Entre na sua conta Google e escolha o backup.",
  hubFileTitle: "Importar do computador",
  hubFileText: "Um backup baixado antes ou copiado do celular.",

  // Google Drive: what to do with the backup
  whatToDo: "O que fazer com o backup?",
  modeTitle: {
    vault: "Importar para o cofre",
    folderAndVault: "Salvar uma cópia numa pasta e importar para o cofre",
    folder: "Só salvar uma cópia numa pasta",
  } satisfies Record<DriveMode, string>,
  modeHint: {
    vault: "Nada fica salvo fora do cofre. Precisa da chave.",
    folderAndVault: "Você escolhe a pasta (HDD, SSD…) e fica com o backup para importar de novo sem o Google. Precisa da chave.",
    folder: "Importe depois, em \"Importar do computador\". Não precisa da chave nem do cofre.",
  } satisfies Record<DriveMode, string>,
  modeButton: {
    vault: "Baixar e importar",
    folderAndVault: "Baixar, salvar e importar",
    folder: "Baixar",
  } satisfies Record<DriveMode, string>,
  folderOnlyNote:
    "Com o cofre fechado, o backup é só salvo numa pasta que você escolhe. Para importá-lo, abra o cofre e use \"Importar do computador\".",

  // App-wide job (download / import in the background)
  jobStarting: "Escolha a pasta onde salvar o backup…",
  filesProgress: (done: string, total: string, bytes: string, totalBytes: string) =>
    `${done} de ${total} arquivos · ${bytes} de ${totalBytes}`,
  countOf: (done: string, total: string) => `${done} de ${total}`,
  backupCopy: "Cópia do backup",
  downloadDone: (downloaded: string, already: string) => `${downloaded} arquivos baixados · ${already} já estavam na pasta`,
  downloadStopped: (reason: string) =>
    `Parou antes do fim: ${reason}. Para continuar, baixe de novo e escolha a mesma pasta: o que já foi baixado não é baixado outra vez.`,
  jobFailed: "Não deu certo",
  jobFinished: "Importação concluída",
  backupSaved: "Backup salvo",
  close: "Fechar",
  details: "Detalhes",
  viewChats: "Ver conversas",
  messagesReady: (app: string) => `As mensagens do ${app} já estão disponíveis. Você pode ler enquanto o resto continua.`,
  resumeHint:
    "O acesso ao Google dura cerca de 1 hora. Se parar antes do fim, entre de novo e escolha a mesma pasta: ele continua de onde parou.",
  saveFolderHint:
    "Você escolhe a pasta onde o backup será salvo, com a mesma estrutura do celular (Databases, Backups, Media). O banco de mensagens continua cifrado; as fotos e vídeos ficam como estão no Google Drive.",
  withFolderMedia: "Importar também as fotos, vídeos e áudios desta pasta",

  // Backup key
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

  // Source
  importAs: "Importar como",

  // Import phases
  phase: {
    reading: "Lendo o arquivo…",
    downloading: "Baixando do Google Drive…",
    decrypting: "Descriptografando…",
    importing: "Importando mensagens e montando o índice de busca…",
    done: "Concluído",
  } satisfies Record<ImportPhase, string>,

  // Most common errors
  friendly: {
    wrong_key: "A chave não abre este backup. Confira os 64 dígitos ou o arquivo de chave.",
    key_mismatch: "Este arquivo de chave é de outro formato de backup.",
    not_msgstore: "O arquivo foi descriptografado, mas não é um banco de mensagens (msgstore).",
  } as Record<string, string>,

  // Summary
  importDone: "Importação concluída",
  summaryExtra: (reactions: string, vcards: string, skipped: string) =>
    `${reactions} reações · ${vcards} contatos compartilhados · ${skipped} status ignorados`,
  unknownTypes: (list: string) =>
    `Tipos de mensagem ainda não reconhecidos: ${list}. Elas aparecem como "mensagem não suportada".`,

  // Media
  media: "Mídias",
  mediaAdded: (added: string, already: string) => `${added} novas · ${already} já estavam no cofre`,
  mediaFailed: (n: string) => ` · ${n} com erro`,
  mediaMatched: (matched: string, expected: string) =>
    `${matched} de ${expected} mídias do histórico encontradas na origem`,
  stoppedEarly: (reason: string) =>
    `Parou antes do fim: ${reason}. O que já entrou continua no cofre; rode de novo para continuar.`,
  stoppedCancelled: "cancelado",
  stoppedExpired: "a sessão do Google expirou; entre de novo para continuar",
  mediaProgress: (done: string, total: string, bytes: string, totalBytes: string, added: string) =>
    `${done} de ${total} arquivos · ${bytes} de ${totalBytes} · ${added} novos`,
  stop: "Parar",

  // Local file import
  dropTitle: "Arraste o msgstore.db.crypt15 ou .crypt14 para cá",
  dropSubtitle: "No celular fica em Android/media/com.whatsapp/WhatsApp/Databases",
  chooseFile: "Escolher arquivo…",
  backupFilter: "Backup do WhatsApp",
  change: "Trocar",
  import: "Importar",
  replaceWarning: (app: string, at: string) =>
    `Já existe um histórico do ${app} importado em ${at}. Ele será substituído por este backup.`,

  // Media folder
  mediaFolderTitle: "Escolha a pasta WhatsApp (ou WhatsApp Business) copiada do celular",
  mediaFolder: "Importar mídias de outra pasta…",

  // Google Drive
  drive: "Google Drive",
  driveFooter: "A sessão do Google foi encerrada e os tokens de acesso foram apagados da memória.",
  ofTotal: (total: string) => ` de ${total}`,
  cancel: "Cancelar",
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
  signOut: "Sair da conta",
  noBackups: "Nenhum backup do WhatsApp encontrado nesta conta.",
  backup: "Backup",
  endToEnd: "ponta a ponta",
  stillUploading: "Ainda sendo enviado pelo celular",
  chatDbSize: (size: string) => `banco de mensagens: ${size}`,
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
  includeMediaHint:
    "Com a chave, as mensagens ficam disponíveis assim que forem importadas; as mídias continuam baixando em segundo plano e depois entram cifradas no cofre.",
  driveDownload: "Baixar do Google Drive",
  introUnofficialBefore: "Usa o mesmo acesso do app do WhatsApp no Android. ",
  introUnofficial: "Não é um método oficial",
  introUnofficialAfter:
    ': o Google pode bloqueá-lo, e você pode receber um alerta de "novo dispositivo" na conta.',
  introMemory:
    "O acesso ao Google fica só na memória deste app, limitado à pasta de backup do WhatsApp, e é apagado ao terminar o download, ao travar o cofre ou depois de 50 minutos.",
  introPassword: "O login acontece numa janela do próprio Google; este app não vê a sua senha.",
  emailLabel: "E-mail da conta Google do backup",
  emailHint: "A janela do Google já abre com este e-mail preenchido.",
  signIn: "Entrar no Google",
  refusedWindow: "O Google recusou a janela de login?",
  manualLogin: "Entrar pelo navegador (login manual)",
};

const en: typeof pt = {
  listTitle: "Choose the backup",
  listSubtitle: (n) => (n === 1 ? "1 backup found in this Google account" : `${n} backups found in this Google account`),
  sectionBackup: "Backup",
  noE2eTag: "not end-to-end encrypted",
  importAsHint: "Google does not say which app each backup belongs to. Check before importing.",
  mediaSwitch: "Photos, videos, audio and documents",
  mediaSwitchSize: (size) => `Up to ${size}`,
  mediaSwitchDetail: "Included in the total backup size",
  driveIntroSubtitle: "Sign in to the Google account that holds the WhatsApp backup.",
  howItWorks: "How it works and what the app can access",
  fileSubtitle: "A backup downloaded earlier or copied from the phone.",
  whereIsTheBackup: "Where to find the backup",
  whereDrive: "Downloaded from Google Drive by WhatsKeep: in the folder you chose, in whatsapp-backup-YYYY-MM-DD/Databases.",
  wherePhone:
    "On an Android phone: Android/media/com.whatsapp/WhatsApp/Databases. Copy the whole WhatsApp folder to bring the photos and videos too.",
  sectionFile: "File",
  withFolderMediaDetail: "From the Media folder next to the file",
  passwordOnlySummary: "My backup uses a password, not a key",
  jobTitleDrive: "Downloading from Google Drive",
  jobTitleLocal: "Importing the backup",
  jobTitleMedia: "Importing media",
  stepLabel: {
    download: "Download the message database",
    import: "Decrypt and import the messages",
    files: "Download photos, videos and audio to the folder",
    media: "Put photos, videos and audio into the vault",
    mediaDownload: "Download photos, videos and audio into the vault",
  },
  donePartial: "Done, with some issues",
  statChats: "chats",
  statMessages: "messages",
  statMedia: "media in the history",
  statMediaInVault: "media in the vault",
  statFiles: "files in the folder",
  statSaved: "saved",
  alsoImported: "Also imported",
  unknownTypesSummary: (types, messages) => `${messages} messages of ${types} types not recognized yet`,

  hubTitle: "How do you want to bring in your history?",
  hubNoVault:
    "The history is kept in an encrypted vault on this computer. The vault password is created when you import.",
  hubDriveTitle: "Download from Google Drive",
  hubDriveText: "Sign in to your Google account and pick the backup.",
  hubFileTitle: "Import from this computer",
  hubFileText: "A backup downloaded earlier or copied from the phone.",

  whatToDo: "What to do with the backup?",
  modeTitle: {
    vault: "Import into the vault",
    folderAndVault: "Save a copy to a folder and import into the vault",
    folder: "Only save a copy to a folder",
  },
  modeHint: {
    vault: "Nothing is saved outside the vault. Needs the key.",
    folderAndVault: "You pick the folder (HDD, SSD…) and keep the backup to import again without Google. Needs the key.",
    folder: 'Import it later, in "Import from this computer". Needs neither the key nor the vault.',
  },
  modeButton: {
    vault: "Download and import",
    folderAndVault: "Download, save and import",
    folder: "Download",
  },
  folderOnlyNote:
    'With the vault closed, the backup is only saved to a folder you choose. To import it, open the vault and use "Import from this computer".',

  jobStarting: "Choose the folder where the backup will be saved…",
  filesProgress: (done, total, bytes, totalBytes) => `${done} of ${total} files · ${bytes} of ${totalBytes}`,
  countOf: (done, total) => `${done} of ${total}`,
  backupCopy: "Backup copy",
  downloadDone: (downloaded, already) => `${downloaded} files downloaded · ${already} already in the folder`,
  downloadStopped: (reason) =>
    `Stopped before the end: ${reason}. To continue, download again and choose the same folder: files already downloaded are not downloaded again.`,
  jobFailed: "It did not work",
  jobFinished: "Import complete",
  backupSaved: "Backup saved",
  close: "Close",
  details: "Details",
  viewChats: "View chats",
  messagesReady: (app) => `The ${app} messages are already available. You can read them while the rest goes on.`,
  resumeHint:
    "Google access lasts about 1 hour. If it stops before the end, sign in again and choose the same folder: it picks up where it left off.",
  saveFolderHint:
    "You choose the folder where the backup is saved, with the same structure as on the phone (Databases, Backups, Media). The message database stays encrypted; photos and videos are kept as they are on Google Drive.",
  withFolderMedia: "Also import the photos, videos and audio from this folder",

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
  summaryExtra: (reactions, vcards, skipped) =>
    `${reactions} reactions · ${vcards} shared contacts · ${skipped} status updates skipped`,
  unknownTypes: (list) => `Message types not recognized yet: ${list}. They show up as "unsupported message".`,

  media: "Media",
  mediaAdded: (added, already) => `${added} new · ${already} already in the vault`,
  mediaFailed: (n) => ` · ${n} failed`,
  mediaMatched: (matched, expected) => `${matched} of ${expected} media files from the history found in the source`,
  stoppedEarly: (reason) =>
    `Stopped before the end: ${reason}. What was already imported stays in the vault; run it again to continue.`,
  stoppedCancelled: "cancelled",
  stoppedExpired: "the Google session expired; sign in again to continue",
  mediaProgress: (done, total, bytes, totalBytes, added) =>
    `${done} of ${total} files · ${bytes} of ${totalBytes} · ${added} new`,
  stop: "Stop",

  dropTitle: "Drag msgstore.db.crypt15 or .crypt14 here",
  dropSubtitle: "On the phone it is in Android/media/com.whatsapp/WhatsApp/Databases",
  chooseFile: "Choose file…",
  backupFilter: "WhatsApp backup",
  change: "Change",
  import: "Import",
  replaceWarning: (app, at) => `A ${app} history imported on ${at} already exists. It will be replaced by this backup.`,

  mediaFolderTitle: "Choose the WhatsApp (or WhatsApp Business) folder copied from the phone",
  mediaFolder: "Import media from another folder…",

  drive: "Google Drive",
  driveFooter: "The Google session was closed and the access tokens were erased from memory.",
  ofTotal: (total) => ` of ${total}`,
  cancel: "Cancel",
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
  signOut: "Sign out",
  noBackups: "No WhatsApp backup found in this account.",
  backup: "Backup",
  endToEnd: "end-to-end",
  stillUploading: "Still being uploaded by the phone",
  chatDbSize: (size) => `message database: ${size}`,
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
  includeMediaHint:
    "With the key, the messages are available as soon as they are imported; the media keep downloading in the background and then go into the vault encrypted.",
  driveDownload: "Download from Google Drive",
  introUnofficialBefore: "Uses the same access as the WhatsApp app on Android. ",
  introUnofficial: "This is not an official method",
  introUnofficialAfter: ': Google may block it, and you may get a "new device" alert on the account.',
  introMemory:
    "Google access stays only in this app's memory, limited to the WhatsApp backup folder, and is erased when the download ends, when the vault is locked or after 50 minutes.",
  introPassword: "Sign-in happens in Google's own window; this app never sees your password.",
  emailLabel: "Email of the Google account with the backup",
  emailHint: "The Google window opens with this email already filled in.",
  signIn: "Sign in to Google",
  refusedWindow: "Did Google refuse the sign-in window?",
  manualLogin: "Sign in through the browser (manual sign-in)",
};

export const useImportT = makeT({ "pt-BR": pt, en });
