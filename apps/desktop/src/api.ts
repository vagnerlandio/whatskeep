// Wrappers tipados dos comandos Rust. O webview nunca vê chaves nem o banco:
// só envia senha/chave uma vez e recebe resumos.
import { convertFileSrc, invoke } from "@tauri-apps/api/core";

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

export type CmdError = { code: string; message: string };

export function errorOf(e: unknown): CmdError {
  if (e && typeof e === "object" && "code" in e && "message" in e) {
    return { code: String(e.code), message: String(e.message) };
  }
  return { code: "internal", message: String(e) };
}

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

export type Person = { jid: string; phone: string | null; name: string | null };

export type Chat = {
  id: number;
  kind: "direct" | "group" | "broadcast" | "channel";
  title: string | null;
  jid: string;
  phone: string | null;
  name: string | null;
  archived: boolean;
  messageCount: number;
  lastMessageAt: number | null;
  last: { kind: MessageKind; text: string | null; fromMe: boolean; sender: Person | null } | null;
};

export type MessageKind =
  | "text"
  | "image"
  | "audio"
  | "voice"
  | "video"
  | "gif"
  | "sticker"
  | "document"
  | "contact"
  | "location"
  | "live_location"
  | "system"
  | "deleted"
  | "unknown";

export type Message = {
  id: number;
  chatId: number;
  sentAt: number;
  fromMe: boolean;
  kind: MessageKind;
  text: string | null;
  sender: Person | null;
  starred: boolean;
  edited: boolean;
  systemAction: number | null;
  quoted: { messageId: number | null; text: string | null; fromMe: boolean; sender: Person | null } | null;
  media: {
    mime: string | null;
    size: number | null;
    width: number | null;
    height: number | null;
    durationS: number | null;
    fileName: string | null;
    transcription: string | null;
    hasThumbnail: boolean;
    /** O arquivo está no cofre. */
    available: boolean;
  } | null;
  location: { latitude: number; longitude: number; name: string | null; address: string | null } | null;
  contacts: { name: string | null; phones: string[] }[];
  reactions: { emoji: string; count: number; mine: boolean }[];
};

export type Cursor = { sentAt: number; id: number };
export type Anchor = { type: "message"; id: number } | { type: "time"; at: number };

export type SearchHit = {
  messageId: number;
  chatId: number;
  sentAt: number;
  fromMe: boolean;
  sender: Person | null;
  /** Trecho com os termos entre \u0001 e \u0002. */
  snippet: string;
};

export const viewerApi = {
  chats: (store: SourceApp) => invoke<Chat[]>("list_chats", { store }),
  messages: (store: SourceApp, chatId: number, direction: "before" | "after", cursor: Cursor | null, limit: number) =>
    invoke<Message[]>("get_messages", { store, chatId, direction, cursor, limit }),
  around: (store: SourceApp, chatId: number, anchor: Anchor, limit: number) =>
    invoke<{ messages: Message[]; anchorIndex: number | null }>("get_messages_around", {
      store,
      chatId,
      anchor,
      limit,
    }),
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

export const MIN_PASSWORD_CHARS = 8;

export function formatSize(bytes: number): string {
  const units = ["B", "KB", "MB", "GB"];
  let v = bytes;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v.toFixed(i === 0 ? 0 : 1)} ${units[i]}`;
}

export const formatCount = (n: number) => n.toLocaleString("pt-BR");
