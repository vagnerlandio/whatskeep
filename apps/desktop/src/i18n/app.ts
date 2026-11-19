// Textos das telas do app (cofre, início, visualizador e exportação HTML).
// Os textos de importação ficam em `import.ts`.
import { formatCount, formatSize } from "@wa/viewer";
import { makeT } from "./errors";

const nPt = (n: number) => formatCount(n, "pt-BR");
const nEn = (n: number) => formatCount(n, "en");
const plPt = (n: number, one: string, many: string) => `${nPt(n)} ${n === 1 ? one : many}`;
const plEn = (n: number, one: string, many: string) => `${nEn(n)} ${n === 1 ? one : many}`;

const pt = {
  // Moldura
  appName: "WhatsKeep",
  tagline: "Suas conversas do WhatsApp, seguras e acessíveis.",
  lockTitle: "Travar o cofre",
  lock: "Travar",
  footnote: "Tudo é processado localmente neste computador. Nada é enviado para servidores.",
  lockedIdle: "O cofre foi travado por inatividade.",

  // Desbloqueio
  unlockTitle: "Cofre travado",
  vaultPassword: "Senha do cofre",
  wrongPassword: "Senha incorreta.",
  opening: "Abrindo…",
  open: "Abrir",

  // Criação do cofre
  createTitle: "Criar cofre",
  createIntro: (minutes: number) =>
    `O histórico importado fica guardado neste computador, cifrado com esta senha. Ela é pedida ao abrir o app e depois de ${minutes} ${minutes === 1 ? "minuto" : "minutos"} sem uso.`,
  password: "Senha",
  passwordRule: (min: number) =>
    `Mínimo de ${min} caracteres. Uma frase longa é mais forte que uma senha curta complicada.`,
  confirmPassword: "Confirmar senha",
  mismatch: "As senhas não conferem.",
  noRecoveryStrong: "Não existe recuperação de senha.",
  noRecoveryCreate: "Se você esquecer, o cofre não pode ser aberto e será preciso importar os backups de novo.",
  creating: "Criando cofre…",

  // Início
  backToHistory: "Voltar ao histórico",
  storesTitle: "Históricos no cofre",
  noStores: "Nenhum backup importado ainda.",
  chats: (n: number) => plPt(n, "conversa", "conversas"),
  messages: (n: number) => plPt(n, "mensagem", "mensagens"),
  media: (n: number) => plPt(n, "mídia", "mídias"),
  importedAt: (when: string) => `Importado em ${when}`,

  // Visualizador
  switchTo: (app: string) => `Trocar para ${app}`,
  importOrExport: "Importar ou exportar",
  emptyTitle: (app: string) => `Histórico do ${app}`,
  emptyNote: "Os dados ficam cifrados neste computador e não saem dele.",

  // Exportação HTML
  exportTitle: "Exportar HTML",
  exportIntro:
    "Gera uma pasta com o histórico que abre em qualquer navegador, sem este app e sem internet. As mensagens ficam cifradas com a senha que você definir.",
  configureExport: "Configurar exportação…",
  exporting: "Exportando",
  exportProgress: (done: number, total: number, messages: number, mediaFiles: number, mediaBytes: number) =>
    `${nPt(done)} de ${plPt(total, "conversa", "conversas")} · ${plPt(messages, "mensagem", "mensagens")} · ${plPt(mediaFiles, "mídia", "mídias")} (${formatSize(mediaBytes, "pt-BR")})`,
  preparing: "Preparando…",
  cancel: "Cancelar",
  exportDone: "Exportação concluída",
  exportSummary: (chats: number, messages: number, mediaFiles: number, mediaBytes: number) =>
    `${plPt(chats, "conversa", "conversas")} · ${plPt(messages, "mensagem", "mensagens")} · ${plPt(mediaFiles, "mídia", "mídias")} (${formatSize(mediaBytes, "pt-BR")})`,
  mediaMissing: (n: number) =>
    n === 1
      ? `${nPt(n)} mídia citada nas mensagens não está no cofre e entrou só como miniatura.`
      : `${nPt(n)} mídias citadas nas mensagens não estão no cofre e entraram só como miniatura.`,
  doneOpen: "Abra o arquivo index.html dessa pasta no navegador e digite a senha.",
  doneCopy: "Para levar a outro computador, copie a pasta inteira.",
  doneNoRecovery: "Não existe recuperação: sem a senha, ninguém abre o histórico (nem você).",
  newExport: "Nova exportação",
  close: "Fechar",
  history: "Histórico",
  chatsLegend: "Conversas",
  allChats: (n: number) => `Todas (${nPt(n)})`,
  pickChats: "Escolher conversas",
  picked: (n: number) => `(${nPt(n)} ${n === 1 ? "escolhida" : "escolhidas"})`,
  filter: "Filtrar",
  pickNote: "As conversas que não forem escolhidas não entram em nada no pacote.",
  passwordLegend: "Senha",
  onePassword: "Uma senha para todas",
  perChatPassword: "Uma senha por conversa (para entregar uma conversa a alguém)",
  passwordPlaceholder: (min: number) => `Senha (mínimo ${min} caracteres)`,
  noRecovery: "Não existe recuperação de senha.",
  mediaLegend: "Fotos, vídeos e áudios",
  mediaEncrypted: "Cifrados (recomendado). No navegador, você seleciona a pasta uma vez para vê-los.",
  mediaPlain: "Sem cifra: abrem direto, mas qualquer pessoa com acesso à pasta vê os arquivos.",
  mediaNone: "Só miniaturas (pacote bem menor)",
  destination: "Destino",
  noFolder: "Nenhuma pasta escolhida",
  chooseFolder: "Escolher pasta…",
  cloudWarning: (cloud: string) =>
    `Esta pasta está no ${cloud}: as mídias sem cifra seriam enviadas para a nuvem. Escolha outra pasta ou use mídias cifradas.`,
  exportButton: (n: number) => `Exportar ${plPt(n, "conversa", "conversas")}`,
};

const en: typeof pt = {
  appName: "WhatsKeep",
  tagline: "Your WhatsApp conversations, safe and accessible.",
  lockTitle: "Lock the vault",
  lock: "Lock",
  footnote: "Everything is processed locally on this computer. Nothing is sent to any server.",
  lockedIdle: "The vault was locked due to inactivity.",

  unlockTitle: "Vault locked",
  vaultPassword: "Vault password",
  wrongPassword: "Wrong password.",
  opening: "Opening…",
  open: "Open",

  createTitle: "Create vault",
  createIntro: (minutes) =>
    `Imported history is stored on this computer, encrypted with this password. You'll be asked for it when opening the app and after ${minutes} ${minutes === 1 ? "minute" : "minutes"} of inactivity.`,
  password: "Password",
  passwordRule: (min) => `At least ${min} characters. A long phrase is stronger than a short, complicated password.`,
  confirmPassword: "Confirm password",
  mismatch: "The passwords don't match.",
  noRecoveryStrong: "There is no password recovery.",
  noRecoveryCreate: "If you forget it, the vault can't be opened and you'll need to import your backups again.",
  creating: "Creating vault…",

  backToHistory: "Back to history",
  storesTitle: "Histories in the vault",
  noStores: "No backup imported yet.",
  chats: (n) => plEn(n, "chat", "chats"),
  messages: (n) => plEn(n, "message", "messages"),
  media: (n) => plEn(n, "media file", "media files"),
  importedAt: (when) => `Imported on ${when}`,

  switchTo: (app) => `Switch to ${app}`,
  importOrExport: "Import or export",
  emptyTitle: (app) => `${app} history`,
  emptyNote: "Your data stays encrypted on this computer and never leaves it.",

  exportTitle: "Export HTML",
  exportIntro:
    "Creates a folder with your history that opens in any browser, without this app and without internet. Messages are encrypted with the password you choose.",
  configureExport: "Set up export…",
  exporting: "Exporting",
  exportProgress: (done, total, messages, mediaFiles, mediaBytes) =>
    `${nEn(done)} of ${plEn(total, "chat", "chats")} · ${plEn(messages, "message", "messages")} · ${plEn(mediaFiles, "media file", "media files")} (${formatSize(mediaBytes, "en")})`,
  preparing: "Preparing…",
  cancel: "Cancel",
  exportDone: "Export complete",
  exportSummary: (chats, messages, mediaFiles, mediaBytes) =>
    `${plEn(chats, "chat", "chats")} · ${plEn(messages, "message", "messages")} · ${plEn(mediaFiles, "media file", "media files")} (${formatSize(mediaBytes, "en")})`,
  mediaMissing: (n) =>
    n === 1
      ? `${nEn(n)} media file referenced in the messages isn't in the vault and was included only as a thumbnail.`
      : `${nEn(n)} media files referenced in the messages aren't in the vault and were included only as thumbnails.`,
  doneOpen: "Open the index.html file in that folder in a browser and enter the password.",
  doneCopy: "To take it to another computer, copy the whole folder.",
  doneNoRecovery: "There is no recovery: without the password, nobody can open the history (not even you).",
  newExport: "New export",
  close: "Close",
  history: "History",
  chatsLegend: "Chats",
  allChats: (n) => `All (${nEn(n)})`,
  pickChats: "Choose chats",
  picked: (n) => `(${nEn(n)} selected)`,
  filter: "Filter",
  pickNote: "Chats that aren't selected are left out of the package entirely.",
  passwordLegend: "Password",
  onePassword: "One password for all",
  perChatPassword: "One password per chat (to hand a chat to someone)",
  passwordPlaceholder: (min) => `Password (at least ${min} characters)`,
  noRecovery: "There is no password recovery.",
  mediaLegend: "Photos, videos and audio",
  mediaEncrypted: "Encrypted (recommended). In the browser, you select the folder once to view them.",
  mediaPlain: "Unencrypted: they open directly, but anyone with access to the folder can see the files.",
  mediaNone: "Thumbnails only (much smaller package)",
  destination: "Destination",
  noFolder: "No folder chosen",
  chooseFolder: "Choose folder…",
  cloudWarning: (cloud) =>
    `This folder is in ${cloud}: unencrypted media would be uploaded to the cloud. Choose another folder or use encrypted media.`,
  exportButton: (n) => `Export ${plEn(n, "chat", "chats")}`,
};

export const useAppT = makeT({ "pt-BR": pt, en });
