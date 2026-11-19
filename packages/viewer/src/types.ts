// Types of the displayed data. They are the same ones Rust serializes
// (wa_vault::query) and the HTML export writes.

export type Person = { jid: string; phone: string | null; name: string | null };

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

export type Media = {
  mime: string | null;
  size: number | null;
  width: number | null;
  height: number | null;
  durationS: number | null;
  fileName: string | null;
  transcription: string | null;
  hasThumbnail: boolean;
  /** The full file is available in this source. */
  available: boolean;
};

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
  media: Media | null;
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
  /** Excerpt with the terms between \u0001 and \u0002. */
  snippet: string;
};

export type Around = { messages: Message[]; anchorIndex: number | null };

export type CmdError = { code: string; message: string };

export function errorOf(e: unknown): CmdError {
  if (e && typeof e === "object" && "code" in e && "message" in e) {
    return { code: String(e.code), message: String(e.message) };
  }
  if (e instanceof Error) return { code: "internal", message: e.message };
  return { code: "internal", message: String(e) };
}

export function formatSize(bytes: number, lang: "pt-BR" | "en" = "pt-BR"): string {
  const units = ["B", "KB", "MB", "GB"];
  let v = bytes;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v.toLocaleString(lang, { maximumFractionDigits: i === 0 ? 0 : 1, minimumFractionDigits: i === 0 ? 0 : 1 })} ${units[i]}`;
}

export const formatCount = (n: number, lang: "pt-BR" | "en" = "pt-BR") => n.toLocaleString(lang);
