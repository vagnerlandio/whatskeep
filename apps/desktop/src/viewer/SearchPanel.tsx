import { useEffect, useRef, useState } from "react";
import { Chat, errorOf, SearchHit, SourceApp, viewerApi } from "../api";
import { chatTitle, formatListTime, Highlight, personLabel } from "./format";

const PAGE = 50;
const DEBOUNCE_MS = 200;

type Props = {
  store: SourceApp;
  query: string;
  /** Busca limitada a uma conversa, ou em todas. */
  scope: Chat | null;
  chatsById: Map<number, Chat>;
  onOpen: (hit: SearchHit) => void;
  onClearScope: () => void;
};

export default function SearchPanel({ store, query, scope, chatsById, onOpen, onClearScope }: Props) {
  const [hits, setHits] = useState<SearchHit[]>([]);
  const [done, setDone] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const reqId = useRef(0);

  useEffect(() => {
    const id = ++reqId.current;
    setError(null);
    const t = setTimeout(async () => {
      setBusy(true);
      try {
        const page = await viewerApi.search(store, query, scope?.id ?? null, null, PAGE);
        if (id !== reqId.current) return; // resposta de uma busca antiga
        setHits(page);
        setDone(page.length < PAGE);
      } catch (e) {
        if (id === reqId.current) setError(errorOf(e).message);
      } finally {
        if (id === reqId.current) setBusy(false);
      }
    }, DEBOUNCE_MS);
    return () => clearTimeout(t);
  }, [store, query, scope?.id]);

  async function more() {
    const last = hits[hits.length - 1];
    if (!last || busy) return;
    const id = reqId.current;
    setBusy(true);
    try {
      const page = await viewerApi.search(store, query, scope?.id ?? null, { sentAt: last.sentAt, id: last.messageId }, PAGE);
      if (id !== reqId.current) return;
      setHits((h) => [...h, ...page]);
      setDone(page.length < PAGE);
    } finally {
      if (id === reqId.current) setBusy(false);
    }
  }

  return (
    <div className="search-panel">
      {scope && (
        <div className="search-scope">
          <span>
            Em <strong>{chatTitle(scope)}</strong>
          </span>
          <button className="link" onClick={onClearScope}>
            buscar em todas
          </button>
        </div>
      )}
      {error && <div className="result error">{error}</div>}
      {!busy && !error && hits.length === 0 && <p className="muted search-empty">Nenhuma mensagem encontrada.</p>}
      <div className="search-results">
        {hits.map((h) => {
          const chat = chatsById.get(h.chatId);
          return (
            <button key={h.messageId} className="search-hit" onClick={() => onOpen(h)}>
              <span className="chat-item-top">
                <span className="chat-item-title">{chat ? chatTitle(chat) : "Conversa"}</span>
                <span className="chat-item-time">{formatListTime(h.sentAt)}</span>
              </span>
              <span className="search-snippet">
                {h.fromMe ? "Você: " : chat?.kind === "group" && h.sender ? `${personLabel(h.sender)}: ` : ""}
                <Highlight text={h.snippet} />
              </span>
            </button>
          );
        })}
        {!done && hits.length > 0 && (
          <button className="more" onClick={more} disabled={busy}>
            {busy ? "Buscando…" : "Mais resultados"}
          </button>
        )}
      </div>
    </div>
  );
}
