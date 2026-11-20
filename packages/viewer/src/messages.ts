// Viewer UI texts, in Portuguese and English.
//
// UI texts only: the content of the backup's messages never goes through
// here (it is shown as it came).
import { makeT, type Lang } from "./i18n";
import { formatCount, type MessageKind } from "./types";

const pt = {
  // navigation rail and sidebar
  chats: "Conversas",
  backToChats: "Voltar às conversas",
  archived: "Arquivadas",
  searchMessages: "Pesquisar mensagens",
  searchThisChat: "Pesquisar nesta conversa",
  clear: "Limpar",
  filterAll: "Tudo",
  filterGroups: "Grupos",
  emptyPaneHint: "Escolha uma conversa ou pesquise por uma mensagem.",

  // chat list
  noChatsHere: "Nenhuma conversa aqui.",
  chatOptions: "Opções da conversa",
  pinChat: "Fixar conversa",
  unpinChat: "Desafixar conversa",
  pinned: "Fixada",
  you: "Você",
  unknownPerson: "Desconhecido",

  // open chat
  group: "Grupo",
  broadcast: "Lista de transmissão",
  channel: "Canal",
  messageCount: (n: number) => `${formatCount(n, "pt-BR")} mensagens`,
  goToDate: "Ir para uma data",
  searchInChat: "Buscar nesta conversa",
  loadingOlder: "Carregando mensagens antigas…",
  backupNote: "Histórico importado de um backup. Fica só neste computador, cifrado.",
  loading: "Carregando…",
  noMessages: "Nenhuma mensagem.",
  goToLatest: "Ir para a mensagem mais recente",
  readOnly: "Somente leitura: este histórico veio de um backup.",

  // bubbles
  media: "Mídia",
  starred: "Favoritada",
  edited: "Editada",
  youDeleted: "Você apagou esta mensagem",
  wasDeleted: "Esta mensagem foi apagada",
  document: "Documento",
  contact: "Contato",
  location: "Localização",
  liveLocation: "Localização em tempo real",
  unsupported: "Este tipo de mensagem ainda não é suportado.",
  readMore: "Ler mais",
  fileNotIncluded: "Arquivo não incluído no histórico",

  // media
  previewNotIncluded: "Prévia · arquivo não incluído",
  play: "Tocar",
  saving: "Salvando…",
  saved: "Salvo",
  save: "Salvar…",
  saveCopy: "Salvar uma cópia",
  close: "Fechar",

  // search
  searchIn: "Em",
  searchAll: "buscar em todas",
  noResults: "Nenhuma mensagem encontrada.",
  messagesSection: "Mensagens",
  chatFallback: "Conversa",
  searching: "Buscando…",
  moreResults: "Mais resultados",

  // link confirmation
  linkTitle: "Abrir link externo?",
  linkExplain: "O link vai abrir no navegador, fora deste histórico. Ele veio de uma mensagem: confira o endereço.",
  linkInvalid: "Este link não pode ser aberto.",
  cancel: "Cancelar",
  open: "Abrir",

  // previews by message kind (chat list)
  kinds: {
    image: "Foto",
    video: "Vídeo",
    gif: "GIF",
    audio: "Áudio",
    voice: "Mensagem de voz",
    sticker: "Figurinha",
    document: "Documento",
    contact: "Contato",
    location: "Localização",
    live_location: "Localização em tempo real",
    deleted: "Mensagem apagada",
    system: "Mensagem do sistema",
    unknown: "Mensagem não suportada",
  } as Partial<Record<MessageKind, string>>,
};

const en: typeof pt = {
  chats: "Chats",
  backToChats: "Back to chats",
  archived: "Archived",
  searchMessages: "Search messages",
  searchThisChat: "Search this chat",
  clear: "Clear",
  filterAll: "All",
  filterGroups: "Groups",
  emptyPaneHint: "Pick a chat or search for a message.",

  noChatsHere: "No chats here.",
  chatOptions: "Chat options",
  pinChat: "Pin chat",
  unpinChat: "Unpin chat",
  pinned: "Pinned",
  you: "You",
  unknownPerson: "Unknown",

  group: "Group",
  broadcast: "Broadcast list",
  channel: "Channel",
  messageCount: (n) => `${formatCount(n, "en")} ${n === 1 ? "message" : "messages"}`,
  goToDate: "Go to a date",
  searchInChat: "Search this chat",
  loadingOlder: "Loading older messages…",
  backupNote: "History imported from a backup. It stays on this computer only, encrypted.",
  loading: "Loading…",
  noMessages: "No messages.",
  goToLatest: "Go to the latest message",
  readOnly: "Read-only: this history came from a backup.",

  media: "Media",
  starred: "Starred",
  edited: "Edited",
  youDeleted: "You deleted this message",
  wasDeleted: "This message was deleted",
  document: "Document",
  contact: "Contact",
  location: "Location",
  liveLocation: "Live location",
  unsupported: "This message type isn't supported yet.",
  readMore: "Read more",
  fileNotIncluded: "File not included in the history",

  previewNotIncluded: "Preview · file not included",
  play: "Play",
  saving: "Saving…",
  saved: "Saved",
  save: "Save…",
  saveCopy: "Save a copy",
  close: "Close",

  searchIn: "In",
  searchAll: "search all chats",
  noResults: "No messages found.",
  messagesSection: "Messages",
  chatFallback: "Chat",
  searching: "Searching…",
  moreResults: "More results",

  linkTitle: "Open external link?",
  linkExplain: "The link will open in your browser, outside this history. It came from a message: check the address.",
  linkInvalid: "This link can't be opened.",
  cancel: "Cancel",
  open: "Open",

  kinds: {
    image: "Photo",
    video: "Video",
    gif: "GIF",
    audio: "Audio",
    voice: "Voice message",
    sticker: "Sticker",
    document: "Document",
    contact: "Contact",
    location: "Location",
    live_location: "Live location",
    deleted: "This message was deleted",
    system: "System message",
    unknown: "Unsupported message",
  },
};

export type ViewerMessages = typeof pt;

/** Dictionaries by language, for functions that aren't components. */
export const VIEWER_MESSAGES: Record<Lang, ViewerMessages> = { "pt-BR": pt, en };

export const useViewerT = makeT(VIEWER_MESSAGES);
