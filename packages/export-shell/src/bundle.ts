// DataSource do export: tudo é decifrado aqui no navegador, sob demanda.
import type { Anchor, Around, Chat, Cursor, DataSource, Lang, Message, SearchHit } from "@wa/viewer";
import { aesKey, b64, hkdf, Kdf, open, openStream, passwordKey, streamLen, unpackChunk } from "./crypto";
import { loadChunk } from "./loader";
import { ShellError, shellMessages } from "./messages";

type Manifest = {
  format: number;
  kdf: Kdf;
  slots: { n: string; c: string }[];
  media: "none" | "encrypted" | "plain";
};

type MediaRef =
  | { p: number; o: number; n: number; id: string; np: string; mime?: string }
  | { f: string; n?: number; mime?: string };

type StoredMessage = Message & { media: (Message["media"] & { thumb?: string; ref?: MediaRef }) | null };

type ChatState = {
  chat: Chat;
  keyId: string;
  chunks: string[];
  master: Uint8Array<ArrayBuffer>;
  key?: CryptoKey;
  keyBytes?: Uint8Array<ArrayBuffer>;
  messages?: StoredMessage[];
  folded?: string[];
  loading?: Promise<StoredMessage[]>;
};

/** Vídeos acima disso não são decifrados para a memória do navegador. */
const MAX_BLOB = 200 * 1024 * 1024;
const URL_CACHE = 60;

export class BundleSource implements DataSource {
  private chatsById = new Map<number, ChatState>();
  private salt: Uint8Array<ArrayBuffer>;
  private packs = new Map<number, File>();
  private urls = new Map<string, string>();
  readonly mediaMode: Manifest["media"];
  /** Idioma dos textos que o visualizador lê daqui (dicas e erros). */
  lang: Lang = "pt-BR";

  constructor(private manifest: Manifest) {
    if (manifest.format !== 1) throw new ShellError("unsupported_format");
    this.salt = b64(manifest.kdf.salt);
    this.mediaMode = manifest.media;
  }

  /** Abre os slots desta senha. Devolve quantas conversas novas foram liberadas. */
  async unlock(password: string): Promise<number> {
    const key = await passwordKey(password, this.manifest.kdf);
    let added = 0;
    for (const slot of this.manifest.slots) {
      let plain: Uint8Array<ArrayBuffer>;
      try {
        plain = await open(key, b64(slot.n), b64(slot.c), "wa-export slot v1");
      } catch {
        continue; // senha de outro slot
      }
      const content = JSON.parse(new TextDecoder().decode(plain)) as {
        master: string;
        chats: { key_id: string; chat: Chat; chunks: string[] }[];
      };
      plain.fill(0);
      const master = b64(content.master);
      for (const e of content.chats) {
        if (this.chatsById.has(e.chat.id)) continue;
        this.chatsById.set(e.chat.id, { chat: normalizeChat(e.chat), keyId: e.key_id, chunks: e.chunks, master });
        added++;
      }
    }
    return added;
  }

  get unlockedCount() {
    return this.chatsById.size;
  }

  get hasMediaFolder() {
    return this.packs.size > 0;
  }

  /** Arquivos da pasta escolhida em <input webkitdirectory>. Devolve quantos pacotes achou. */
  setFolder(files: FileList | File[]): number {
    this.packs.clear();
    for (const f of Array.from(files)) {
      const m = /(?:^|\/)media\/pack-(\d{3})\.bin$/.exec(f.webkitRelativePath || f.name);
      if (m) this.packs.set(Number(m[1]), f);
    }
    return this.packs.size;
  }

  private async chatKey(s: ChatState): Promise<CryptoKey> {
    if (!s.key) {
      s.keyBytes = await hkdf(s.master, this.salt, `wa-export chat v1:${s.keyId}`);
      s.key = await aesKey(s.keyBytes);
    }
    return s.key;
  }

  private load(chatId: number): Promise<StoredMessage[]> {
    const s = this.chatsById.get(chatId);
    if (!s) return Promise.reject(new ShellError("chat_locked", "", this.lang));
    if (s.messages) return Promise.resolve(s.messages);
    s.loading ??= (async () => {
      const key = await this.chatKey(s);
      const all: StoredMessage[] = [];
      for (const id of s.chunks) {
        const blob = b64(await loadChunk(id).catch((e) => Promise.reject(e instanceof ShellError ? e.in(this.lang) : e)));
        const plain = await open(key, blob.subarray(0, 12), blob.subarray(12), id);
        const json = (await unpackChunk(plain)) as { messages: Partial<StoredMessage>[] };
        for (const m of json.messages) all.push(normalizeMessage(m));
      }
      all.sort((a, b) => a.sentAt - b.sentAt || a.id - b.id);
      s.messages = all;
      return all;
    })();
    return s.loading;
  }

  // ---------------------------------------------------------------- DataSource

  async chats(): Promise<Chat[]> {
    return [...this.chatsById.values()]
      .map((s) => s.chat)
      .sort((a, b) => (b.lastMessageAt ?? 0) - (a.lastMessageAt ?? 0) || b.id - a.id);
  }

  async messages(chatId: number, direction: "before" | "after", cursor: Cursor | null, limit: number) {
    const all = await this.load(chatId);
    if (direction === "before") {
      const end = cursor ? lowerBound(all, cursor) : all.length;
      return all.slice(Math.max(0, end - limit), end);
    }
    const start = cursor ? upperBound(all, cursor) : 0;
    return all.slice(start, start + limit);
  }

  async around(chatId: number, anchor: Anchor, limit: number): Promise<Around> {
    const all = await this.load(chatId);
    let idx = anchor.type === "message" ? all.findIndex((m) => m.id === anchor.id) : all.findIndex((m) => m.sentAt >= anchor.at);
    if (idx < 0 && anchor.type === "time") idx = all.length - 1;
    if (idx < 0) return { messages: [], anchorIndex: null };
    const half = Math.floor(limit / 2);
    const start = Math.max(0, idx - half);
    return { messages: all.slice(start, idx + half + 1), anchorIndex: idx - start };
  }

  async search(query: string, chatId: number | null, before: Cursor | null, limit: number): Promise<SearchHit[]> {
    const terms = fold(query).split(/\s+/).filter(Boolean);
    if (!terms.length) return [];
    const ids = chatId != null ? [chatId] : [...this.chatsById.keys()];
    const found: { m: StoredMessage; chatId: number }[] = [];
    for (const id of ids) {
      const s = this.chatsById.get(id);
      if (!s) continue;
      const all = await this.load(id);
      s.folded ??= all.map((m) => (m.text && m.kind !== "deleted" ? fold(m.text) : ""));
      for (let i = all.length - 1; i >= 0; i--) {
        const m = all[i];
        if (before && (m.sentAt > before.sentAt || (m.sentAt === before.sentAt && m.id >= before.id))) continue;
        const t = s.folded[i];
        if (t && terms.every((term) => t.includes(term))) found.push({ m, chatId: id });
        if (i % 20000 === 0) await new Promise((r) => setTimeout(r)); // não trava a tela
      }
    }
    found.sort((a, b) => b.m.sentAt - a.m.sentAt || b.m.id - a.m.id);
    // O trecho destacado só é montado para a página exibida.
    return found.slice(0, limit).map(({ m, chatId }) => ({
      messageId: m.id,
      chatId,
      sentAt: m.sentAt,
      fromMe: m.fromMe,
      sender: m.sender,
      snippet: snippet(m.text ?? "", terms),
    }));
  }

  thumbUrl(m: Message): string | null {
    const thumb = (m as StoredMessage).media?.thumb;
    if (!thumb) return null;
    return this.cachedUrl(`t${m.chatId}:${m.id}`, () => new Blob([b64(thumb)], { type: "image/jpeg" }));
  }

  mediaUrl(m: Message): string | null | Promise<string | null> {
    const ref = (m as StoredMessage).media?.ref;
    if (!ref) return null;
    if ("f" in ref) {
      // Modo sem cifra: arquivo ao lado do index.html (nome validado).
      return /^media\/[0-9a-f]{16}$/.test(ref.f) ? ref.f : null;
    }
    const pack = this.packs.get(ref.p);
    if (!pack || ref.n > MAX_BLOB) return null;
    const cacheKey = `m${m.chatId}:${m.id}`;
    const hit = this.urls.get(cacheKey);
    if (hit) return hit;
    return (async () => {
      const s = this.chatsById.get(m.chatId)!;
      await this.chatKey(s);
      const raw = await hkdf(s.keyBytes!, this.salt, `wa-export media v1:${ref.id}`);
      const key = await aesKey(raw);
      raw.fill(0);
      const read = async (off: number, len: number) =>
        new Uint8Array(await pack.slice(ref.o + off, ref.o + off + len).arrayBuffer());
      if (ref.o + streamLen(ref.n) > pack.size) throw new ShellError("media_truncated", "", this.lang);
      const parts = await openStream(key, b64(ref.np), ref.id, read, ref.n);
      return this.cachedUrl(cacheKey, () => new Blob(parts, { type: safeMime(ref.mime) }));
    })();
  }

  async saveMedia(m: Message): Promise<boolean> {
    const url = await this.mediaUrl(m);
    if (!url) return false;
    const a = document.createElement("a");
    a.href = url;
    a.download = safeName(m);
    a.click();
    return true;
  }

  mediaHint(): string | null {
    if (this.mediaMode === "encrypted" && !this.hasMediaFolder) {
      return shellMessages[this.lang].hintPickFolder;
    }
    return shellMessages[this.lang].hintTooLarge;
  }

  /** Esquece chaves e mídias decifradas (travar). */
  dispose() {
    for (const url of this.urls.values()) URL.revokeObjectURL(url);
    this.urls.clear();
    for (const s of this.chatsById.values()) {
      s.master.fill(0);
      s.keyBytes?.fill(0);
    }
    this.chatsById.clear();
  }

  private cachedUrl(key: string, make: () => Blob): string {
    const hit = this.urls.get(key);
    if (hit) {
      this.urls.delete(key);
      this.urls.set(key, hit); // LRU
      return hit;
    }
    const url = URL.createObjectURL(make());
    this.urls.set(key, url);
    if (this.urls.size > URL_CACHE) {
      const [oldKey, oldUrl] = this.urls.entries().next().value!;
      this.urls.delete(oldKey);
      URL.revokeObjectURL(oldUrl);
    }
    return url;
  }
}

// ---------------------------------------------------------------- utilidades

function cmp(m: { sentAt: number; id: number }, c: Cursor) {
  return m.sentAt - c.sentAt || m.id - c.id;
}

/** Primeiro índice com mensagem >= cursor. */
function lowerBound(all: Message[], c: Cursor) {
  let lo = 0;
  let hi = all.length;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (cmp(all[mid], c) < 0) lo = mid + 1;
    else hi = mid;
  }
  return lo;
}

/** Primeiro índice com mensagem > cursor. */
function upperBound(all: Message[], c: Cursor) {
  let lo = 0;
  let hi = all.length;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (cmp(all[mid], c) <= 0) lo = mid + 1;
    else hi = mid;
  }
  return lo;
}

/** Minúsculas e sem acentos. */
export function fold(s: string): string {
  return s.normalize("NFD").replace(/\p{M}/gu, "").toLowerCase();
}

/** Trecho em volta do primeiro termo, com cada ocorrência entre \u0001 e \u0002. */
export function snippet(text: string, terms: string[]): string {
  // Dobra caractere a caractere, guardando de onde veio cada posição dobrada.
  const chars = Array.from(text);
  let folded = "";
  const origin: number[] = [];
  chars.forEach((c, i) => {
    const f = fold(c);
    folded += f;
    for (let k = 0; k < f.length; k++) origin.push(i);
  });
  const marks = new Array<boolean>(chars.length).fill(false);
  let first = -1;
  for (const term of terms) {
    let from = 0;
    for (let at = folded.indexOf(term, from); at >= 0; at = folded.indexOf(term, from)) {
      for (let k = at; k < at + term.length; k++) marks[origin[k]] = true;
      if (first < 0 || origin[at] < first) first = origin[at];
      from = at + term.length;
    }
  }
  const start = Math.max(0, (first < 0 ? 0 : first) - 40);
  const end = Math.min(chars.length, start + 140);
  let out = start > 0 ? "…" : "";
  for (let i = start; i < end; i++) {
    if (marks[i] && (i === start || !marks[i - 1])) out += "\u0001";
    out += chars[i];
    if (marks[i] && (i + 1 === end || !marks[i + 1])) out += "\u0002";
  }
  if (end < chars.length) out += "…";
  return out.replace(/[\r\n]+/g, " ");
}

function safeMime(mime?: string): string {
  const m = (mime ?? "").split(";")[0].trim().toLowerCase();
  return /^(image\/(jpeg|png|webp|gif)|video\/(mp4|3gpp|quicktime)|audio\/(ogg|mpeg|mp4|aac|amr))$/.test(m)
    ? m
    : "application/octet-stream";
}

function safeName(m: Message): string {
  const raw = m.media?.fileName ?? `midia-${m.id}`;
  return raw.replace(/[\u0000-\u001f/\\:*?"<>|]/g, "_").replace(/^\.+/, "").slice(0, 150) || `midia-${m.id}`;
}

function normalizeChat(c: Partial<Chat>): Chat {
  return {
    id: c.id!,
    kind: c.kind ?? "direct",
    title: c.title ?? null,
    jid: c.jid ?? "",
    phone: c.phone ?? null,
    name: c.name ?? null,
    archived: !!c.archived,
    messageCount: c.messageCount ?? 0,
    lastMessageAt: c.lastMessageAt ?? null,
    last: c.last
      ? { kind: c.last.kind, text: c.last.text ?? null, fromMe: !!c.last.fromMe, sender: c.last.sender ?? null }
      : null,
  };
}

/** O export omite campos nulos/falsos/vazios; aqui eles voltam aos padrões. */
function normalizeMessage(m: Partial<StoredMessage>): StoredMessage {
  return {
    id: m.id!,
    chatId: m.chatId!,
    sentAt: m.sentAt ?? 0,
    fromMe: !!m.fromMe,
    kind: m.kind ?? "unknown",
    text: m.text ?? null,
    sender: m.sender ? { jid: m.sender.jid, phone: m.sender.phone ?? null, name: m.sender.name ?? null } : null,
    starred: !!m.starred,
    edited: !!m.edited,
    systemAction: m.systemAction ?? null,
    quoted: m.quoted
      ? {
          messageId: m.quoted.messageId ?? null,
          text: m.quoted.text ?? null,
          fromMe: !!m.quoted.fromMe,
          sender: m.quoted.sender ?? null,
        }
      : null,
    media: m.media
      ? {
          mime: m.media.mime ?? null,
          size: m.media.size ?? null,
          width: m.media.width ?? null,
          height: m.media.height ?? null,
          durationS: m.media.durationS ?? null,
          fileName: m.media.fileName ?? null,
          transcription: m.media.transcription ?? null,
          hasThumbnail: !!m.media.thumb,
          available: !!m.media.available,
          thumb: m.media.thumb,
          ref: m.media.ref,
        }
      : null,
    location: m.location
      ? { latitude: m.location.latitude, longitude: m.location.longitude, name: m.location.name ?? null, address: m.location.address ?? null }
      : null,
    contacts: (m.contacts ?? []).map((c) => ({ name: c.name ?? null, phones: c.phones ?? [] })),
    reactions: (m.reactions ?? []).map((r) => ({ emoji: r.emoji, count: r.count ?? 1, mine: !!r.mine })),
  };
}
