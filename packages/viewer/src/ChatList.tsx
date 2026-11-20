import { memo, useEffect, useRef, useState } from "react";
import { Virtuoso } from "react-virtuoso";
import type { Chat } from "./types";
import Avatar from "./Avatar";
import { chatTitle, formatListTime, personLabel, Preview } from "./format";
import { Icon } from "./icons";
import { useLang } from "./i18n";
import { useViewerT } from "./messages";

type Props = {
  chats: Chat[];
  selectedId: number | null;
  onSelect: (chat: Chat) => void;
  archivedCount: number;
  showingArchived: boolean;
  onToggleArchived: () => void;
  /** Pins/unpins (absent: no chat menu, e.g. in the export). */
  onTogglePin?: (chat: Chat) => void;
};

/** Where the chat menu opens: at the chevron, or where the user right-clicked. */
type MenuAt = { chat: Chat; x: number; y: number };

export default function ChatList({
  chats,
  selectedId,
  onSelect,
  archivedCount,
  showingArchived,
  onToggleArchived,
  onTogglePin,
}: Props) {
  const t = useViewerT();
  const [menu, setMenu] = useState<MenuAt | null>(null);
  const openMenu = onTogglePin ? setMenu : undefined;
  return (
    <div className="chat-list">
      {archivedCount > 0 && !showingArchived && (
        <button className="archived-row" onClick={onToggleArchived}>
          <span className="archived-icon">
            <Icon name="archive" size={20} />
          </span>
          <span className="archived-label">{t.archived}</span>
          <span className="count">{archivedCount}</span>
        </button>
      )}
      {chats.length === 0 && <p className="muted list-empty">{t.noChatsHere}</p>}
      <Virtuoso
        className="chat-list-scroll"
        data={chats}
        computeItemKey={(_, c) => c.id}
        itemContent={(_, c) => (
          <ChatItem chat={c} selected={c.id === selectedId} onSelect={onSelect} onMenu={openMenu} menuOpen={menu?.chat.id === c.id} />
        )}
      />
      {menu && onTogglePin && (
        <ChatMenu
          at={menu}
          onClose={() => setMenu(null)}
          onTogglePin={() => {
            onTogglePin(menu.chat);
            setMenu(null);
          }}
        />
      )}
    </div>
  );
}

const ChatItem = memo(function ChatItem({
  chat,
  selected,
  onSelect,
  onMenu,
  menuOpen,
}: {
  chat: Chat;
  selected: boolean;
  onSelect: (c: Chat) => void;
  onMenu?: (m: MenuAt) => void;
  menuOpen: boolean;
}) {
  // Hooks in here: memo doesn't prevent re-rendering when the language changes.
  const t = useViewerT();
  const { lang } = useLang();
  const last = chat.last;
  const prefix = !last
    ? ""
    : last.fromMe && last.kind !== "system"
      ? `${t.you}: `
      : chat.kind === "group" && last.sender && last.kind !== "system"
        ? `${personLabel(last.sender, lang)}: `
        : "";
  return (
    <div className={`chat-item-wrap ${menuOpen ? "menu-open" : ""}`}>
      <button
        className={`chat-item ${selected ? "selected" : ""}`}
        onClick={() => onSelect(chat)}
        onContextMenu={
          onMenu &&
          ((e) => {
            e.preventDefault();
            onMenu({ chat, x: e.clientX, y: e.clientY });
          })
        }
      >
        <Avatar chat={chat} />
        <span className="chat-item-body">
          <span className="chat-item-top">
            <span className="chat-item-title">{chatTitle(chat)}</span>
            {chat.lastMessageAt != null && <span className="chat-item-time">{formatListTime(chat.lastMessageAt, lang)}</span>}
          </span>
          <span className="chat-item-preview">
            {last && (
              <>
                {prefix && <span className="preview-prefix">{prefix}</span>}
                <Preview kind={last.kind} text={last.text} />
              </>
            )}
            {chat.pinned && (
              <span className="chat-item-pin" title={t.pinned}>
                <Icon name="pin" size={15} />
              </span>
            )}
          </span>
        </span>
      </button>
      {onMenu && (
        <button
          className="chat-item-menu"
          title={t.chatOptions}
          aria-label={t.chatOptions}
          aria-haspopup="menu"
          onClick={(e) => {
            const r = e.currentTarget.getBoundingClientRect();
            onMenu({ chat, x: r.left, y: r.bottom + 4 });
          }}
        >
          <Icon name="chevronDown" size={18} />
        </button>
      )}
    </div>
  );
});

/** Small menu next to a chat. Fixed position (the list scrolls and clips). */
function ChatMenu({ at, onClose, onTogglePin }: { at: MenuAt; onClose: () => void; onTogglePin: () => void }) {
  const t = useViewerT();
  const ref = useRef<HTMLDivElement>(null);
  const first = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    first.current?.focus();
    const onDown = (e: PointerEvent) => !ref.current?.contains(e.target as Node) && onClose();
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("pointerdown", onDown, true);
    window.addEventListener("keydown", onKey);
    window.addEventListener("resize", onClose);
    return () => {
      window.removeEventListener("pointerdown", onDown, true);
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("resize", onClose);
    };
  }, [onClose]);

  // Keeps the menu inside the window.
  const left = Math.min(at.x, window.innerWidth - 220);
  const top = Math.min(at.y, window.innerHeight - 60);
  return (
    <div ref={ref} className="chat-menu" role="menu" style={{ left, top }}>
      <button ref={first} role="menuitem" onClick={onTogglePin}>
        <Icon name={at.chat.pinned ? "pinOff" : "pin"} size={18} />
        {at.chat.pinned ? t.unpinChat : t.pinChat}
      </button>
    </div>
  );
}
