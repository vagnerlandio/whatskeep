import { createContext, useContext, useEffect, useState } from "react";
import type { Anchor, Around, Chat, Cursor, Message, SearchHit } from "./types";

/** Uma URL pronta (`wamedia://…`, `blob:…`, caminho relativo) ou a promessa dela. */
export type UrlResult = string | null | Promise<string | null>;

/**
 * De onde o visualizador tira os dados. O app implementa sobre o cofre (Rust,
 * via IPC); o export HTML, sobre os pedaços cifrados decifrados no navegador.
 */
export interface DataSource {
  chats(): Promise<Chat[]>;
  /** Página em ordem cronológica, antes ou depois do cursor. */
  messages(chatId: number, direction: "before" | "after", cursor: Cursor | null, limit: number): Promise<Message[]>;
  around(chatId: number, anchor: Anchor, limit: number): Promise<Around>;
  /** Do mais recente para o mais antigo; `before` continua a partir do último resultado. */
  search(query: string, chatId: number | null, before: Cursor | null, limit: number): Promise<SearchHit[]>;
  thumbUrl(m: Message): UrlResult;
  /** Arquivo completo. `null` se não estiver acessível agora. */
  mediaUrl(m: Message): UrlResult;
  /** Salvar uma cópia. Ausente se a fonte não permitir. */
  saveMedia?(m: Message): Promise<boolean>;
  /** Por que `mediaUrl` não está disponível (ex.: falta escolher a pasta). */
  mediaHint?(): string | null;
  /** Abre um link http(s) fora do visualizador (já confirmado pelo usuário). */
  openLink?(url: string): Promise<void>;
}

export const SourceContext = createContext<DataSource | null>(null);

export function useSource(): DataSource {
  const s = useContext(SourceContext);
  if (!s) throw new Error("DataSource ausente");
  return s;
}

/** Resolve uma `UrlResult` (síncrona ou não) para uso em `src`. */
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
