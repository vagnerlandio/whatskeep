import { ReactNode, useEffect, useMemo, useRef, useState } from "react";
import ChatList from "./ChatList";
import ChatPane from "./ChatPane";
import SearchPanel from "./SearchPanel";
import { DataSource, SourceContext } from "./source";
import { Anchor, Chat, errorOf, SearchHit } from "./types";
import { Icon, IconName } from "./icons";
import { LanguageSelect } from "./i18n";
import { useViewerT } from "./messages";

type Props = {
  source: DataSource;
  /** Sidebar title (e.g. "WhatsApp"). */
  title: ReactNode;
  /** Buttons next to the title. */
  actions?: ReactNode;
  /** Extra buttons on the left navigation rail, at the top and the bottom. */
  railTop?: ReactNode;
  railBottom?: ReactNode;
  /** Notice above the chat list (e.g. "select the media folder"). */
  banner?: ReactNode;
  emptyTitle: string;
  emptyNote?: string;
};

type Filter = "all" | "groups";

/** Navigation rail + chat list (and search) + open chat, in the
 * WhatsApp Desktop layout. Change the `key` to reset when switching sources. */
export default function Viewer({ source, title, actions, railTop, railBottom, banner, emptyTitle, emptyNote }: Props) {
  const [chats, setChats] = useState<Chat[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [showArchived, setShowArchived] = useState(false);
  const [filter, setFilter] = useState<Filter>("all");
  const [selected, setSelected] = useState<Chat | null>(null);
  const [anchor, setAnchor] = useState<Anchor | null>(null);
  const [query, setQuery] = useState("");
  const [scope, setScope] = useState<Chat | null>(null);
  const searchInput = useRef<HTMLInputElement>(null);
  const t = useViewerT();

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
  const visible = useMemo(
    () => chats.filter((c) => c.archived === showArchived && (showArchived || filter === "all" || c.kind === "group")),
    [chats, showArchived, filter],
  );
  const archivedCount = useMemo(() => chats.filter((c) => c.archived).length, [chats]);
  const hasGroups = useMemo(() => chats.some((c) => c.kind === "group"), [chats]);

  function openChat(chat: Chat, at: Anchor | null = null) {
    setSelected(chat);
    setAnchor(at);
  }

  function openHit(hit: SearchHit) {
    const chat = chatsById.get(hit.chatId);
    // A new object on every click: reopens at the message even if it's the same one.
    if (chat) openChat(chat, { type: "message", id: hit.messageId });
  }

  function searchInChat() {
    if (!selected) return;
    setScope(selected);
    searchInput.current?.focus();
  }

  function showChats() {
    setShowArchived(false);
    setQuery("");
    setScope(null);
  }

  const searching = query.trim().length > 0;

  return (
    <SourceContext.Provider value={source}>
      <div className="viewer">
        <nav className="rail">
          <RailButton icon="chats" title={t.chats} active={!showArchived && !searching} onClick={showChats} />
          {railTop}
          <span className="rail-spacer" />
          <LanguageSelect variant="rail" />
          {railBottom}
        </nav>

        <aside className="sidebar">
          <header className="sidebar-header">
            {showArchived && (
              <button className="icon-btn" title={t.backToChats} onClick={() => setShowArchived(false)}>
                <Icon name="back" size={22} />
              </button>
            )}
            <h1 className={showArchived ? "sidebar-title plain" : "sidebar-title"}>{showArchived ? t.archived : title}</h1>
            {actions && <div className="sidebar-actions">{actions}</div>}
          </header>

          <div className="search-box">
            <Icon name="search" size={18} />
            <input
              ref={searchInput}
              type="search"
              placeholder={scope ? t.searchThisChat : t.searchMessages}
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
            {query && (
              <button className="icon-btn tiny" title={t.clear} onClick={() => (setQuery(""), setScope(null))}>
                <Icon name="close" size={16} />
              </button>
            )}
          </div>

          {!searching && !showArchived && hasGroups && (
            <div className="chips">
              <button className={filter === "all" ? "chip active" : "chip"} onClick={() => setFilter("all")}>
                {t.filterAll}
              </button>
              <button className={filter === "groups" ? "chip active" : "chip"} onClick={() => setFilter("groups")}>
                {t.filterGroups}
              </button>
            </div>
          )}

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
              <div className="empty-icon">
                <Icon name="chats" size={56} strokeWidth={1.25} />
              </div>
              <h2>{emptyTitle}</h2>
              <p className="muted">{t.emptyPaneHint}</p>
              {emptyNote && (
                <p className="empty-note">
                  <Icon name="lock" size={14} />
                  {emptyNote}
                </p>
              )}
            </div>
          )}
        </main>
      </div>
    </SourceContext.Provider>
  );
}

/** Button on the left navigation rail. */
export function RailButton({
  icon,
  title,
  onClick,
  active,
  badge,
}: {
  icon: IconName;
  title: string;
  onClick: () => void;
  active?: boolean;
  badge?: string;
}) {
  return (
    <button className={active ? "rail-btn active" : "rail-btn"} title={title} aria-label={title} onClick={onClick}>
      <Icon name={icon} size={22} />
      {badge && <span className="rail-badge">{badge}</span>}
    </button>
  );
}
