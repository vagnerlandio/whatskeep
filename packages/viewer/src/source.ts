import { createContext, useContext, useEffect, useState } from "react";
import type { Anchor, Around, Chat, Cursor, Message, SearchHit } from "./types";

/** A ready URL (`wkmedia://…`, `blob:…`, relative path) or a promise of one. */
export type UrlResult = string | null | Promise<string | null>;

/**
 * Where the viewer gets its data from. The app implements it on top of the vault
 * (Rust, via IPC); the HTML export, on top of the encrypted chunks decrypted in the browser.
 */
export interface DataSource {
  chats(): Promise<Chat[]>;
  /** A page in chronological order, before or after the cursor. */
  messages(chatId: number, direction: "before" | "after", cursor: Cursor | null, limit: number): Promise<Message[]>;
  around(chatId: number, anchor: Anchor, limit: number): Promise<Around>;
  /** Newest to oldest; `before` continues from the last result. */
  search(query: string, chatId: number | null, before: Cursor | null, limit: number): Promise<SearchHit[]>;
  thumbUrl(m: Message): UrlResult;
  /** Full file. `null` if it isn't accessible right now. */
  mediaUrl(m: Message): UrlResult;
  /** Save a copy. Absent if the source doesn't allow it. */
  saveMedia?(m: Message): Promise<boolean>;
  /** Why `mediaUrl` isn't available (e.g. the folder hasn't been chosen yet). */
  mediaHint?(): string | null;
  /** Opens an http(s) link outside the viewer (already confirmed by the user). */
  openLink?(url: string): Promise<void>;
}

export const SourceContext = createContext<DataSource | null>(null);

export function useSource(): DataSource {
  const s = useContext(SourceContext);
  if (!s) throw new Error("DataSource missing");
  return s;
}

/** Resolves a `UrlResult` (sync or not) for use in `src`. */
export function useUrl(get: () => UrlResult, deps: unknown[]): string | null {
  const [url, setUrl] = useState<string | null>(() => {
    const r = get();
    return typeof r === "string" || r === null ? r : null;
  });
  useEffect(() => {
    let alive = true;
    const r = get();
    if (r instanceof Promise) {
      r.then((u) => alive && setUrl(u)).catch(() => alive && setUrl(null));
    } else {
      setUrl(r);
    }
    return () => {
      alive = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, deps);
  return url;
}
