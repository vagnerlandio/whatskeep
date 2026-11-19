// Wrappers tipados dos comandos Rust. O webview nunca vê chaves nem o banco:
// só envia senha/chave uma vez e recebe resumos.
import { invoke } from "@tauri-apps/api/core";

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

export type ImportPhase = "reading" | "decrypting" | "importing" | "done";

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
