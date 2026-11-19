import { useEffect, useMemo, useRef, useState } from "react";
import { Anchor, Chat, errorOf, SearchHit, SOURCE_LABEL, SourceApp, StoreInfo, viewerApi } from "../api";
import ChatList from "./ChatList";
import ChatPane from "./ChatPane";
import SearchPanel from "./SearchPanel";

type Props = {
  stores: StoreInfo[];
  onLock: () => void;
  onOpenImport: () => void;
};

export default function Viewer({ stores, onLock, onOpenImport }: Props) {
  const [store, setStore] = useState<SourceApp>(stores[0].sourceApp);
  const [chats, setChats] = useState<Chat[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [showArchived, setShowArchived] = useState(false);
  const [selected, setSelected] = useState<Chat | null>(null);
  const [anchor, setAnchor] = useState<Anchor | null>(null);
  const [query, setQuery] = useState("");
  const [scope, setScope] = useState<Chat | null>(null);
  const searchInput = useRef<HTMLInputElement>(null);

  // Se o banco selecionado sumir (não deveria), volta para o primeiro.
  useEffect(() => {
    if (!stores.some((s) => s.sourceApp === store)) setStore(stores[0].sourceApp);
  }, [stores, store]);

  const storeStamp = stores.find((s) => s.sourceApp === store)?.importedAtMs;
  useEffect(() => {
    let cancelled = false;
    setSelected(null);
    setScope(null);
    setQuery("");
    viewerApi
      .chats(store)
      .then((c) => !cancelled && (setChats(c), setError(null)))
      .catch((e) => !cancelled && setError(errorOf(e).message));
    return () => {
      cancelled = true;
    };
  }, [store, storeStamp]);

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
    <div className="viewer">
      <aside className="sidebar">
        <header className="sidebar-header">
          {stores.length > 1 ? (
            <div className="store-switch">
              {stores.map((s) => (
                <button
                  key={s.sourceApp}
                  className={s.sourceApp === store ? "active" : ""}
                  onClick={() => setStore(s.sourceApp)}
                >
                  {s.sourceApp === "business" ? "Business" : "WhatsApp"}
                </button>
              ))}
            </div>
          ) : (
            <strong className="sidebar-title">{SOURCE_LABEL[store]}</strong>
          )}
          <div className="sidebar-actions">
            <button className="icon-btn" title="Importar backup" onClick={onOpenImport}>
              ＋
            </button>
            <button className="icon-btn" title="Travar o cofre" onClick={onLock}>
              🔒
            </button>
          </div>
        </header>

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

        {error && <div className="result error">{error}</div>}
        {searching ? (
          <SearchPanel
            store={store}
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
          <ChatPane
            key={`${store}:${selected.id}`}
            store={store}
            chat={selected}
            anchor={anchor}
            onSearchInChat={searchInChat}
          />
        ) : (
          <div className="empty-pane">
            <div className="empty-icon">💬</div>
            <h2>Histórico do {SOURCE_LABEL[store]}</h2>
            <p className="muted">Escolha uma conversa ou pesquise por uma mensagem.</p>
            <p className="muted small">🔒 Os dados ficam cifrados neste computador e não saem dele.</p>
          </div>
        )}
      </main>
    </div>
  );
}
