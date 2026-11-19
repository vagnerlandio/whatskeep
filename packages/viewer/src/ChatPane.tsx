import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Virtuoso, VirtuosoHandle } from "react-virtuoso";
import { useSource } from "./source";
import { Anchor, Chat, errorOf, Message } from "./types";
import { chatTitle, formatPhone } from "./format";
import Avatar from "./Avatar";
import MessageRow from "./MessageRow";
import { Lightbox } from "./MediaView";
import LinkConfirm from "./LinkConfirm";
import { ViewerContext } from "./context";
import { Icon } from "./icons";
import { useViewerT } from "./messages";

const PAGE = 80;
/** Virtual index high enough to keep prepending older pages. */
const START = 1_000_000_000;

type Loaded = {
  messages: Message[];
  firstItemIndex: number;
  hasOlder: boolean;
  hasNewer: boolean;
  initialIndex: number;
  centered: boolean;
};

type Props = {
  chat: Chat;
  /** Anchor coming from outside (search result). */
  anchor: Anchor | null;
  onSearchInChat: () => void;
};

export default function ChatPane({ chat, anchor: externalAnchor, onSearchInChat }: Props) {
  const source = useSource();
  const [anchor, setAnchor] = useState<Anchor | null>(externalAnchor);
  const [seq, setSeq] = useState(0); // forces reloading/remounting the list
  const [data, setData] = useState<Loaded | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [highlightId, setHighlightId] = useState<number | null>(null);
  const [atBottom, setAtBottom] = useState(true);
  const [scrolling, setScrolling] = useState(false);
  const [lightbox, setLightbox] = useState<Message | null>(null);
  const [link, setLink] = useState<string | null>(null);
  const viewerCtx = useMemo(() => ({ scrolling, openMedia: setLightbox, openLink: setLink }), [scrolling]);
  const loading = useRef(false);
  const virtuoso = useRef<VirtuosoHandle>(null);
  const t = useViewerT();

  // New anchor from outside with the chat already open (the Viewer remounts this
  // component when switching chats, so the first anchor comes from the initial state).
  const mounted = useRef(false);
  useEffect(() => {
    if (!mounted.current) {
      mounted.current = true;
      return;
    }
    setAnchor(externalAnchor);
    setSeq((s) => s + 1);
  }, [externalAnchor]);

  useEffect(() => {
    let cancelled = false;
    setData(null);
    setError(null);
    (async () => {
      try {
        let loaded: Loaded;
        if (anchor) {
          const r = await source.around(chat.id, anchor, PAGE);
          const idx = r.anchorIndex ?? Math.max(0, r.messages.length - 1);
          loaded = {
            messages: r.messages,
            firstItemIndex: START,
            hasOlder: idx >= PAGE / 2,
            hasNewer: r.messages.length - idx - 1 >= PAGE / 2,
            initialIndex: idx,
            centered: true,
          };
          setHighlightId(anchor.type === "message" ? anchor.id : (r.messages[idx]?.id ?? null));
        } else {
          const msgs = await source.messages(chat.id, "before", null, PAGE);
          loaded = {
            messages: msgs,
            firstItemIndex: START,
            hasOlder: msgs.length === PAGE,
            hasNewer: false,
            initialIndex: Math.max(0, msgs.length - 1),
            centered: false,
          };
          setHighlightId(null);
        }
        if (!cancelled) setData(loaded);
      } catch (e) {
        if (!cancelled) setError(errorOf(e).message);
      }
    })();
    return () => {
      cancelled = true;
    };
    // `seq` changes on every navigation (even to the same anchor).
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [seq]);

  // The highlight fades after a while.
  useEffect(() => {
    if (highlightId === null) return;
    const t = setTimeout(() => setHighlightId(null), 2500);
    return () => clearTimeout(t);
  }, [highlightId]);

  const loadOlder = useCallback(async () => {
    if (!data?.hasOlder || loading.current) return;
    loading.current = true;
    try {
      const first = data.messages[0];
      const older = await source.messages(chat.id, "before", { sentAt: first.sentAt, id: first.id }, PAGE);
      setData((d) =>
        d && {
          ...d,
          messages: [...older, ...d.messages],
          firstItemIndex: d.firstItemIndex - older.length,
          hasOlder: older.length === PAGE,
        },
      );
    } finally {
      loading.current = false;
    }
  }, [data, source, chat.id]);

  const loadNewer = useCallback(async () => {
    if (!data?.hasNewer || loading.current) return;
    loading.current = true;
    try {
      const last = data.messages[data.messages.length - 1];
      const newer = await source.messages(chat.id, "after", { sentAt: last.sentAt, id: last.id }, PAGE);
      setData((d) => d && { ...d, messages: [...d.messages, ...newer], hasNewer: newer.length === PAGE });
    } finally {
      loading.current = false;
    }
  }, [data, source, chat.id]);

  const jumpTo = useCallback((next: Anchor | null) => {
    setAnchor(next);
    setSeq((s) => s + 1);
  }, []);

  const onJumpToMessage = useCallback((id: number) => jumpTo({ type: "message", id }), [jumpTo]);

  function goToEnd() {
    if (data && !data.hasNewer) {
      virtuoso.current?.scrollToIndex({ index: "LAST", align: "end", behavior: "auto" });
    } else {
      jumpTo(null);
    }
  }

  function onPickDate(value: string) {
    if (!value) return;
    const [y, m, d] = value.split("-").map(Number);
    jumpTo({ type: "time", at: new Date(y, m - 1, d).getTime() });
  }

  const isGroup = chat.kind === "group";
  const subtitle =
    chat.kind === "group"
      ? t.group
      : chat.kind === "broadcast"
        ? t.broadcast
        : chat.kind === "channel"
          ? t.channel
          : chat.phone && chat.title
            ? formatPhone(chat.phone)
            : null;

  const components = useMemo(
    () => ({
      Header: () =>
        data?.hasOlder ? (
          <div className="list-edge">{t.loadingOlder}</div>
        ) : (
          <div className="list-edge">
            <span className="e2e-note">
              <Icon name="lock" size={12} />
              {t.backupNote}
            </span>
          </div>
        ),
      Footer: () => (data?.hasNewer ? <div className="list-edge">{t.loading}</div> : <div style={{ height: 12 }} />),
    }),
    [data?.hasOlder, data?.hasNewer, t],
  );

  return (
    <section className="chat-pane">
      <header className="chat-header">
        <Avatar chat={chat} size={40} />
        <div className="chat-header-text">
          <strong>{chatTitle(chat)}</strong>
          <span>
            {[subtitle, t.messageCount(chat.messageCount)].filter(Boolean).join(" · ")}
          </span>
        </div>
        <label className="icon-btn" title={t.goToDate}>
          <Icon name="calendar" size={22} />
          <input type="date" className="date-input" onChange={(e) => onPickDate(e.target.value)} />
        </label>
        <button className="icon-btn" title={t.searchInChat} onClick={onSearchInChat}>
          <Icon name="search" size={22} />
        </button>
      </header>

      <ViewerContext.Provider value={viewerCtx}>
      <div className="messages-wrap">
        {error && <div className="result error pane-error">{error}</div>}
        {data && data.messages.length === 0 && <div className="list-edge">{t.noMessages}</div>}
        {data && data.messages.length > 0 && (
          <Virtuoso
            key={seq}
            ref={virtuoso}
            className="messages"
            data={data.messages}
            firstItemIndex={data.firstItemIndex}
            initialTopMostItemIndex={{ index: data.initialIndex, align: data.centered ? "center" : "end" }}
            alignToBottom
            startReached={loadOlder}
            endReached={loadNewer}
            atBottomStateChange={setAtBottom}
            isScrolling={setScrolling}
            increaseViewportBy={{ top: 800, bottom: 800 }}
            computeItemKey={(_, m) => m.id}
            components={components}
            itemContent={(index, m) => (
              <MessageRow
                message={m}
                prev={data.messages[index - data.firstItemIndex - 1]}
                isGroup={isGroup}
                highlighted={m.id === highlightId}
                onJump={onJumpToMessage}
              />
            )}
          />
        )}
        {data && (!atBottom || data.hasNewer) && (
          <button className="to-bottom" onClick={goToEnd} title={t.goToLatest}>
            <Icon name="chevronDown" size={24} />
          </button>
        )}
      </div>
      </ViewerContext.Provider>
      <footer className="chat-footer">
        <div className="readonly-bar">
          <Icon name="lock" size={16} />
          {t.readOnly}
        </div>
      </footer>
      {lightbox && <Lightbox m={lightbox} onClose={() => setLightbox(null)} />}
      {link && <LinkConfirm url={link} onClose={() => setLink(null)} />}
    </section>
  );
}
