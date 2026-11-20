// Typed wrappers for the Rust commands. The webview never sees keys or the database:
// it only sends the password/key once and receives summaries.
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import type { Anchor, Around, Chat, Cursor, DataSource, Lang, Message, SearchHit } from "@wk/viewer";
export { errorOf, formatCount, formatSize } from "@wk/viewer";
export type { CmdError } from "@wk/viewer";

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

export type BackupInfo = {
  version: "crypt14" | "crypt15";
  sizeBytes: number;
  /** The file is in a backup folder with a `Media/` folder next to it. */
  mediaFolder: boolean;
};

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
  /** Bytes the vault takes on disk. */
  usage: () => invoke<number>("vault_usage"),
  /** Deletes the whole vault (open or locked). */
  deleteVault: () => invoke<void>("vault_delete"),
  /** Deletes one app's history (messages and media). */
  deleteStore: (store: SourceApp) => invoke<void>("store_delete", { store }),
  inspectBackup: (path: string) => invoke<BackupInfo>("inspect_backup", { path }),
  importBackup: (backupPath: string, key: KeyInput, sourceApp: SourceApp, withMedia: boolean) =>
    invoke<{ summary: ImportSummary; media: MediaSummary | null }>("import_backup", {
      backupPath,
      key,
      sourceApp,
      withMedia,
    }),
};

// ---------------------------------------------------------------- viewer

export const viewerApi = {
  chats: (store: SourceApp) => invoke<Chat[]>("list_chats", { store }),
  messages: (store: SourceApp, chatId: number, direction: "before" | "after", cursor: Cursor | null, limit: number) =>
    invoke<Message[]>("get_messages", { store, chatId, direction, cursor, limit }),
  around: (store: SourceApp, chatId: number, anchor: Anchor, limit: number) =>
    invoke<Around>("get_messages_around", { store, chatId, anchor, limit }),
  search: (store: SourceApp, query: string, chatId: number | null, before: Cursor | null, limit: number) =>
    invoke<SearchHit[]>("search_messages", { store, query, chatId, before, limit }),
};

/** Thumbnail served by the `wkmedia://` protocol (straight from the vault, no disk cache). */
export const thumbUrl = (store: SourceApp, messageId: number) =>
  convertFileSrc(`thumb/${store}/${messageId}`, "wkmedia");

/** Full file (with Range support for video/audio). */
export const mediaUrl = (store: SourceApp, messageId: number) =>
  convertFileSrc(`media/${store}/${messageId}`, "wkmedia");

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
  /** Opens the folder picker in the backend. `null` if the user cancels. */
  importFolder: (sourceApp: SourceApp) => invoke<MediaSummary | null>("import_media_folder", { sourceApp }),
  cancel: () => invoke<void>("media_cancel"),
  /** Opens "Save as" in the backend. `false` if the user cancels. */
  save: (store: SourceApp, messageId: number) => invoke<boolean>("save_media", { store, messageId }),
};

// ---------------------------------------------------------------- Google Drive

export type DriveBackup = {
  /** Index in the backend session (the backup's real name never leaves Rust). */
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

/** `gdrive-progress`: the database byte by byte, then the other files. */
export type DownloadProgress = {
  phase: "database" | "files";
  filesDone: number;
  filesTotal: number;
  bytes: number;
  totalBytes: number;
  failed: number;
};

export type DownloadSummary = {
  files: number;
  downloaded: number;
  already: number;
  failed: number;
  bytes: number;
  totalBytes: number;
  stopped: string | null;
};

export type DriveResult = {
  /** Where the backup was saved (not in the "vault" mode). */
  folder: string | null;
  download: DownloadSummary | null;
  /** When the backup went into the vault. */
  summary: ImportSummary | null;
  media: MediaSummary | null;
};

/** Straight into the vault · a copy in a folder and then the vault · only the copy. */
export type DriveMode = "vault" | "folderAndVault" | "folder";

export const driveApi = {
  login: (email: string) => invoke<DriveLogin>("gdrive_login", { email }),
  loginManual: (email: string, oauthToken: string) => invoke<DriveLogin>("gdrive_login_manual", { email, oauthToken }),
  cancel: () => invoke<void>("gdrive_cancel"),
  /** Folder modes open the folder picker in the backend: `null` if the user cancels it. */
  download: (backupId: number, sourceApp: SourceApp, mode: DriveMode, key: KeyInput | null, includeMedia: boolean) =>
    invoke<DriveResult | null>("gdrive_download", { backupId, sourceApp, mode, key, includeMedia }),
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
  /** Native folder picker; the path is kept in the backend. */
  pickDestination: () => invoke<{ path: string; cloud: string | null } | null>("export_pick_destination"),
  run: (store: SourceApp, groups: { password: string; chats: number[] }[], media: ExportMedia) =>
    invoke<{ summary: ExportSummary; path: string }>("export_html", { store, groups, media }),
  cancel: () => invoke<void>("export_cancel"),
};

export const MIN_PASSWORD_CHARS = 8;

// ---------------------------------------------------------------- preferences

export const settingsApi = {
  /** Saved language, or `null` (use the system one). */
  getLanguage: () => invoke<Lang | null>("get_language"),
  setLanguage: (language: Lang) => invoke<void>("set_language", { language }),
};



/** Viewer data source on top of the vault (Rust, via IPC). */
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
