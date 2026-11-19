// Wrappers tipados dos comandos Rust. O webview nunca vê chaves nem o banco:
// só envia senha/chave uma vez e recebe resumos.
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import type { Anchor, Around, Chat, Cursor, DataSource, Message, SearchHit } from "@wa/viewer";
export { errorOf, formatCount, formatSize } from "@wa/viewer";
export type { CmdError } from "@wa/viewer";

export type VaultState = "none" | "locked" | "unlocked";
export type VaultStatus = { state: VaultState; autolockSecs: number };

export type SourceApp = "whatsapp" | "business";
export const SOURCE_LABEL: Record<SourceApp, string> = {
  whatsapp: "WhatsApp",
  business: "WhatsApp Business",
};

export type StoreInfo = {
  sourceApp: SourceApp;
  importedAtMs: number;
  sourceSchema: string;
  chats: number;
  messages: number;
};

export type BackupInfo = { version: "crypt14" | "crypt15"; sizeBytes: number };

export type KeyInput = { type: "hex"; value: string } | { type: "file"; path: string };

export type ImportSummary = {
  sourceSchema: "modern" | "legacy" | null;
  chats: number;
  messages: number;
  media: number;
  reactions: number;
  vcards: number;
  skippedStatus: number;
  skippedOrphans: number;
  unknownTypes: [number, number][];
};

export type ImportPhase = "reading" | "downloading" | "decrypting" | "importing" | "done";


export const api = {
  status: () => invoke<VaultStatus>("vault_status"),
  create: (password: string) => invoke<void>("vault_create", { password }),
  unlock: (password: string) => invoke<void>("vault_unlock", { password }),
  lock: () => invoke<void>("vault_lock"),
  touch: () => invoke<void>("vault_touch"),
  changePassword: (current: string, next: string) => invoke<void>("vault_change_password", { current, next }),
  listStores: () => invoke<StoreInfo[]>("list_stores"),
  inspectBackup: (path: string) => invoke<BackupInfo>("inspect_backup", { path }),
  importBackup: (backupPath: string, key: KeyInput, sourceApp: SourceApp) =>
    invoke<ImportSummary>("import_backup", { backupPath, key, sourceApp }),
};

// ---------------------------------------------------------------- visualizador

export const viewerApi = {
  chats: (store: SourceApp) => invoke<Chat[]>("list_chats", { store }),
  messages: (store: SourceApp, chatId: number, direction: "before" | "after", cursor: Cursor | null, limit: number) =>
    invoke<Message[]>("get_messages", { store, chatId, direction, cursor, limit }),
  around: (store: SourceApp, chatId: number, anchor: Anchor, limit: number) =>
    invoke<Around>("get_messages_around", { store, chatId, anchor, limit }),
  search: (store: SourceApp, query: string, chatId: number | null, before: Cursor | null, limit: number) =>
    invoke<SearchHit[]>("search_messages", { store, query, chatId, before, limit }),
};

/** Miniatura servida pelo protocolo `wamedia://` (direto do cofre, sem cache em disco). */
export const thumbUrl = (store: SourceApp, messageId: number) =>
  convertFileSrc(`thumb/${store}/${messageId}`, "wamedia");

/** Arquivo completo (com suporte a Range para vídeo/áudio). */
export const mediaUrl = (store: SourceApp, messageId: number) =>
  convertFileSrc(`media/${store}/${messageId}`, "wamedia");

export type MediaSummary = {
  matched: number;
  added: number;
  already: number;
  unmatched: number;
  failed: number;
  bytes: number;
  expected: number;
  stopped: string | null;
};

export type MediaProgress = { done: number; total: number; bytes: number; totalBytes: number; added: number };

export const mediaApi = {
  /** Abre o seletor de pasta no backend. `null` se o usuário cancelar. */
  importFolder: (sourceApp: SourceApp) => invoke<MediaSummary | null>("import_media_folder", { sourceApp }),
  cancel: () => invoke<void>("media_cancel"),
  /** Abre "Salvar como" no backend. `false` se o usuário cancelar. */
  save: (store: SourceApp, messageId: number) => invoke<boolean>("save_media", { store, messageId }),
};

// ---------------------------------------------------------------- Google Drive

export type DriveBackup = {
  /** Índice na sessão do backend (o nome real do backup não sai do Rust). */
  id: number;
  phone: string | null;
  updateTime: string | null;
  sizeBytes: number | null;
  uploading: boolean;
  encrypted: boolean | null;
  chatDbSize: number | null;
  mediaSize: number | null;
  messages: number | null;
  appVersion: string | null;
  apps: SourceApp[];
};

export type DriveLogin = { backups: DriveBackup[]; refused: SourceApp[] };

export const driveApi = {
  login: (email: string) => invoke<DriveLogin>("gdrive_login", { email }),
  loginManual: (email: string, oauthToken: string) => invoke<DriveLogin>("gdrive_login_manual", { email, oauthToken }),
  cancel: () => invoke<void>("gdrive_cancel"),
  download: (backupId: number, key: KeyInput, sourceApp: SourceApp, includeMedia: boolean) =>
    invoke<{ summary: ImportSummary; media: MediaSummary | null }>("gdrive_download", {
      backupId,
      key,
      sourceApp,
      includeMedia,
    }),
};

// ---------------------------------------------------------------- export HTML

export type ExportMedia = "encrypted" | "plain" | "none";
export type ExportProgress = { chatsDone: number; chatsTotal: number; messages: number; mediaFiles: number; mediaBytes: number };
export type ExportSummary = {
  chats: number;
  messages: number;
  mediaFiles: number;
  mediaBytes: number;
  mediaMissing: number;
  slots: number;
};

export const exportApi = {
  /** Seletor nativo de pasta; o caminho fica guardado no backend. */
  pickDestination: () => invoke<{ path: string; cloud: string | null } | null>("export_pick_destination"),
  run: (store: SourceApp, groups: { password: string; chats: number[] }[], media: ExportMedia) =>
    invoke<{ summary: ExportSummary; path: string }>("export_html", { store, groups, media }),
  cancel: () => invoke<void>("export_cancel"),
};

export const MIN_PASSWORD_CHARS = 8;



/** Fonte de dados do visualizador sobre o cofre (Rust, via IPC). */
export function tauriSource(store: SourceApp): DataSource {
  return {
    chats: () => viewerApi.chats(store),
    messages: (chatId, direction, cursor, limit) => viewerApi.messages(store, chatId, direction, cursor, limit),
    around: (chatId, anchor, limit) => viewerApi.around(store, chatId, anchor, limit),
    search: (query, chatId, before, limit) => viewerApi.search(store, query, chatId, before, limit),
    thumbUrl: (m) => thumbUrl(store, m.id),
    mediaUrl: (m) => mediaUrl(store, m.id),
    saveMedia: (m) => mediaApi.save(store, m.id),
    openLink: (url) => invoke<void>("open_link", { url }),
  };
}
