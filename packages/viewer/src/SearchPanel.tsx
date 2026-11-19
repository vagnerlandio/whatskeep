import { useEffect, useRef, useState } from "react";
import { useSource } from "./source";
import { Chat, errorOf, SearchHit } from "./types";
import { chatTitle, formatListTime, Highlight, personLabel } from "./format";
import { useLang } from "./i18n";
import { useViewerT } from "./messages";

const PAGE = 50;
const DEBOUNCE_MS = 200;

type Props = {
  query: string;
  /** Search limited to one chat, or across all. */
  scope: Chat | null;
  chatsById: Map<number, Chat>;
  onOpen: (hit: SearchHit) => void;
  onClearScope: () => void;
};

export default function SearchPanel({ query, scope, chatsById, onOpen, onClearScope }: Props) {
  const source = useSource();
  const t = useViewerT();
  const { lang } = useLang();
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
        const page = await source.search(query, scope?.id ?? null, null, PAGE);
        if (id !== reqId.current) return; // response from an old search
        setHits(page);
        setDone(page.length < PAGE);
      } catch (e) {
        if (id === reqId.current) setError(errorOf(e).message);
      } finally {
        if (id === reqId.current) setBusy(false);
      }
    }, DEBOUNCE_MS);
    return () => clearTimeout(t);
  }, [source, query, scope?.id]);

  async function more() {
    const last = hits[hits.length - 1];
    if (!last || busy) return;
    const id = reqId.current;
    setBusy(true);
    try {
      const page = await source.search(query, scope?.id ?? null, { sentAt: last.sentAt, id: last.messageId }, PAGE);
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
            {t.searchIn} <strong>{chatTitle(scope)}</strong>
          </span>
          <button className="link" onClick={onClearScope}>
            {t.searchAll}
          </button>
        </div>
      )}
      {error && <div className="result error">{error}</div>}
      {!busy && !error && hits.length === 0 && <p className="muted search-empty">{t.noResults}</p>}
      {hits.length > 0 && <div className="search-section">{t.messagesSection}</div>}
      <div className="search-results">
        {hits.map((h) => {
          const chat = chatsById.get(h.chatId);
          return (
            <button key={h.messageId} className="search-hit" onClick={() => onOpen(h)}>
              <span className="chat-item-top">
                <span className="chat-item-title">{chat ? chatTitle(chat) : t.chatFallback}</span>
                <span className="chat-item-time">{formatListTime(h.sentAt, lang)}</span>
              </span>
              <span className="search-snippet">
                {h.fromMe ? `${t.you}: ` : chat?.kind === "group" && h.sender ? `${personLabel(h.sender, lang)}: ` : ""}
                <Highlight text={h.snippet} />
              </span>
            </button>
          );
        })}
        {!done && hits.length > 0 && (
          <button className="more" onClick={more} disabled={busy}>
            {busy ? t.searching : t.moreResults}
          </button>
        )}
      </div>
    </div>
  );
}
