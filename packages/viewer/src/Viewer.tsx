import { ReactNode, useEffect, useMemo, useRef, useState } from "react";
import ChatList from "./ChatList";
import ChatPane from "./ChatPane";
import SearchPanel from "./SearchPanel";
import { DataSource, SourceContext } from "./source";
import { Anchor, Chat, errorOf, SearchHit } from "./types";

type Props = {
  source: DataSource;
  /** Conteúdo do topo da barra lateral (título, botões do app). */
  header: ReactNode;
  /** Aviso acima da lista de conversas (ex.: "selecione a pasta das mídias"). */
  banner?: ReactNode;
  emptyTitle: string;
  emptyNote?: string;
};

/** Barra lateral (conversas e busca) + conversa aberta. Troque o `key` para
 * reiniciar ao trocar de fonte. */
export default function Viewer({ source, header, banner, emptyTitle, emptyNote }: Props) {
  const [chats, setChats] = useState<Chat[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [showArchived, setShowArchived] = useState(false);
  const [selected, setSelected] = useState<Chat | null>(null);
  const [anchor, setAnchor] = useState<Anchor | null>(null);
  const [query, setQuery] = useState("");
  const [scope, setScope] = useState<Chat | null>(null);
  const searchInput = useRef<HTMLInputElement>(null);

  useEffect(() => {
    let cancelled = false;
    source
      .chats()
      .then((c) => !cancelled && (setChats(c), setError(null)))
      .catch((e) => !cancelled && setError(errorOf(e).message));
    return () => {
      cancelled = true;
    };
  }, [source]);

  const chatsById = useMemo(() => new Map(chats.map((c) => [c.id, c])), [chats]);
  const visible = useMemo(() => chats.filter((c) => c.archived === showArchived), [chats, showArchived]);
  const archivedCount = useMemo(() => chats.filter((c) => c.archived).length, [chats]);

  function openChat(chat: Chat, at: Anchor | null = null) {
    setSelected(chat);
    setAnchor(at);
  }

  function openHit(hit: SearchHit) {
    const chat = chatsById.get(hit.chatId);
    // Objeto novo a cada clique: reabre na mensagem mesmo que seja a mesma.
    if (chat) openChat(chat, { type: "message", id: hit.messageId });
  }

  function searchInChat() {
    if (!selected) return;
    setScope(selected);
    searchInput.current?.focus();
  }

  const searching = query.trim().length > 0;

  return (
    <SourceContext.Provider value={source}>
      <div className="viewer">
        <aside className="sidebar">
          <header className="sidebar-header">{header}</header>

          <div className="search-box">
            <span aria-hidden>🔍</span>
            <input
              ref={searchInput}
              type="search"
              placeholder={scope ? "Pesquisar nesta conversa" : "Pesquisar mensagens"}
              value={query}
              spellCheck={false}
              onChange={(e) => setQuery(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Escape") {
                  setQuery("");
                  setScope(null);
                }
              }}
            />
          </div>

          {banner}
          {error && <div className="result error">{error}</div>}
          {searching ? (
            <SearchPanel
              query={query}
              scope={scope}
              chatsById={chatsById}
              onOpen={openHit}
              onClearScope={() => setScope(null)}
            />
          ) : (
            <ChatList
              chats={visible}
              selectedId={selected?.id ?? null}
              onSelect={(c) => openChat(c)}
              archivedCount={archivedCount}
              showingArchived={showArchived}
              onToggleArchived={() => setShowArchived((v) => !v)}
            />
          )}
        </aside>

        <main className="main-pane">
          {selected ? (
            <ChatPane key={selected.id} chat={selected} anchor={anchor} onSearchInChat={searchInChat} />
          ) : (
            <div className="empty-pane">
              <div className="empty-icon">💬</div>
              <h2>{emptyTitle}</h2>
              <p className="muted">Escolha uma conversa ou pesquise por uma mensagem.</p>
              {emptyNote && <p className="muted small">{emptyNote}</p>}
            </div>
          )}
        </main>
      </div>
    </SourceContext.Provider>
  );
}
