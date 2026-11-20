import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { useSource } from "./source";
import { Anchor, Chat, errorOf, Message } from "./types";
import { chatTitle, formatPhone } from "./format";
import Avatar from "./Avatar";
import MessageRow from "./MessageRow";
import { Lightbox } from "./MediaView";
import LinkConfirm from "./LinkConfirm";
import { ScrollWatch, ViewerContext } from "./context";
import { Icon } from "./icons";
import { useViewerT } from "./messages";

const PAGE = 80;
/** Messages kept in the page; the far end is dropped (and loaded again if needed). */
const MAX_LOADED = 400;
/** Distance from an end (px) at which the next page is loaded. */
const EDGE = 1500;

/*
 * The list is not virtualized: every loaded message is in the page with its real
 * height. Virtualized lists estimate the height of the messages that come into view
 * and correct the scroll position a frame later when they measure them; with
 * messages as varied as a chat's, that correction showed as the list shaking while
 * scrolling up. Here nothing is estimated: when a page is added or dropped, the
 * first visible message is put back where it was before the frame is painted
 * (`useLayoutEffect`), so the content does not move.
 */

type Loaded = {
  messages: Message[];
  hasOlder: boolean;
  hasNewer: boolean;
};

/** Where to put the view on the first render of a (re)load. */
type Initial = { id: number | null; centered: boolean };

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
  const [lightbox, setLightbox] = useState<Message | null>(null);
  const [link, setLink] = useState<string | null>(null);
  const scroll = useMemo(() => new ScrollWatch(), []);
  const viewerCtx = useMemo(() => ({ scroll, openMedia: setLightbox, openLink: setLink }), [scroll]);
  const loading = useRef(false);
  const scroller = useRef<HTMLDivElement>(null);
  /** Placement for the first render after a (re)load. */
  const initial = useRef<Initial | null>(null);
  /** A message and its offset from the top of the view, to keep across the next render. */
  const keep = useRef<{ id: number; top: number } | null>(null);
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
        let place: Initial;
        if (anchor) {
          const r = await source.around(chat.id, anchor, PAGE);
          const idx = r.anchorIndex ?? Math.max(0, r.messages.length - 1);
          loaded = {
            messages: r.messages,
            hasOlder: idx >= PAGE / 2,
            hasNewer: r.messages.length - idx - 1 >= PAGE / 2,
          };
          place = { id: r.messages[idx]?.id ?? null, centered: true };
          setHighlightId(anchor.type === "message" ? anchor.id : (r.messages[idx]?.id ?? null));
        } else {
          const msgs = await source.messages(chat.id, "before", null, PAGE);
          loaded = { messages: msgs, hasOlder: msgs.length === PAGE, hasNewer: false };
          place = { id: null, centered: false };
          setHighlightId(null);
        }
        if (!cancelled) {
          initial.current = place;
          setData(loaded);
        }
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

  /** Remembers the first visible message and where it is, to keep it there. */
  const holdPosition = useCallback(() => {
    const el = scroller.current;
    if (!el) return;
    const top = el.getBoundingClientRect().top;
    for (const row of el.querySelectorAll<HTMLElement>("[data-mid]")) {
      const r = row.getBoundingClientRect();
      if (r.bottom > top) {
        keep.current = { id: Number(row.dataset.mid), top: r.top - top };
        return;
      }
    }
  }, []);

  const loadOlder = useCallback(async () => {
    if (!data?.hasOlder || loading.current) return;
    loading.current = true;
    try {
      const first = data.messages[0];
      const older = await source.messages(chat.id, "before", { sentAt: first.sentAt, id: first.id }, PAGE);
      holdPosition();
      setData((d) => {
        if (!d) return d;
        const all = [...older, ...d.messages];
        const cut = all.length > MAX_LOADED;
        return { messages: cut ? all.slice(0, MAX_LOADED) : all, hasOlder: older.length === PAGE, hasNewer: d.hasNewer || cut };
      });
    } finally {
      loading.current = false;
    }
  }, [data, source, chat.id, holdPosition]);

  const loadNewer = useCallback(async () => {
    if (!data?.hasNewer || loading.current) return;
    loading.current = true;
    try {
      const last = data.messages[data.messages.length - 1];
      const newer = await source.messages(chat.id, "after", { sentAt: last.sentAt, id: last.id }, PAGE);
      holdPosition();
      setData((d) => {
        if (!d) return d;
        const all = [...d.messages, ...newer];
        const cut = all.length > MAX_LOADED;
        return { messages: cut ? all.slice(all.length - MAX_LOADED) : all, hasOlder: d.hasOlder || cut, hasNewer: newer.length === PAGE };
      });
    } finally {
      loading.current = false;
    }
  }, [data, source, chat.id, holdPosition]);

  // Before the frame is painted: the first placement after a load, or the kept
  // message back where it was (pages added or dropped above it).
  useLayoutEffect(() => {
    const el = scroller.current;
    if (!el || !data) return;
    const place = initial.current;
    if (place) {
      initial.current = null;
      keep.current = null;
      const row = place.id === null ? null : el.querySelector<HTMLElement>(`[data-mid="${place.id}"]`);
      el.scrollTop = row ? row.offsetTop - el.clientHeight / 2 + row.offsetHeight / 2 : el.scrollHeight;
      return;
    }
    const k = keep.current;
    keep.current = null;
    const row = k && el.querySelector<HTMLElement>(`[data-mid="${k.id}"]`);
    if (k && row) el.scrollTop += row.getBoundingClientRect().top - el.getBoundingClientRect().top - k.top;
  }, [data]);

  /** Loads the next page when the view gets near an end (also right after a load). */
  const checkEdges = useCallback(() => {
    const el = scroller.current;
    if (!el) return;
    const fromBottom = el.scrollHeight - el.scrollTop - el.clientHeight;
    setAtBottom(fromBottom < 40);
    if (el.scrollTop < EDGE) void loadOlder();
    else if (fromBottom < EDGE) void loadNewer();
  }, [loadOlder, loadNewer]);

  useEffect(() => {
    checkEdges();
  }, [checkEdges]);

  const jumpTo = useCallback((next: Anchor | null) => {
    setAnchor(next);
    setSeq((s) => s + 1);
  }, []);

  const onJumpToMessage = useCallback((id: number) => jumpTo({ type: "message", id }), [jumpTo]);

  function goToEnd() {
    const el = scroller.current;
    if (data && !data.hasNewer && el) {
      el.scrollTop = el.scrollHeight;
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
          <div
            key={seq}
            ref={scroller}
            className="messages"
            onScroll={() => {
              scroll.touch();
              checkEdges();
            }}
          >
            <div className="messages-inner">
              {data.hasOlder ? (
                <div className="list-edge">{t.loadingOlder}</div>
              ) : (
                <div className="list-edge">
                  <span className="e2e-note">
                    <Icon name="lock" size={12} />
                    {t.backupNote}
                  </span>
                </div>
              )}
              {data.messages.map((m, i) => (
                <div key={m.id} data-mid={m.id}>
                  <MessageRow
                    message={m}
                    prev={data.messages[i - 1]}
                    isGroup={isGroup}
                    highlighted={m.id === highlightId}
                    onJump={onJumpToMessage}
                  />
                </div>
              ))}
              {data.hasNewer ? <div className="list-edge">{t.loading}</div> : <div style={{ height: 12 }} />}
            </div>
          </div>
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
