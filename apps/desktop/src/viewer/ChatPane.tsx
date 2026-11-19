import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Virtuoso, VirtuosoHandle } from "react-virtuoso";
import { Anchor, Chat, errorOf, formatCount, Message, SourceApp, viewerApi } from "../api";
import { chatTitle, formatPhone } from "./format";
import Avatar from "./Avatar";
import MessageRow from "./MessageRow";
import { Lightbox } from "./MediaView";
import { ViewerContext } from "./context";

const PAGE = 80;
/** Índice virtual alto o bastante para ir prependendo páginas antigas. */
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
  store: SourceApp;
  chat: Chat;
  /** Âncora vinda de fora (resultado de busca). */
  anchor: Anchor | null;
  onSearchInChat: () => void;
};

export default function ChatPane({ store, chat, anchor: externalAnchor, onSearchInChat }: Props) {
  const [anchor, setAnchor] = useState<Anchor | null>(externalAnchor);
  const [seq, setSeq] = useState(0); // força recarregar/remontar a lista
  const [data, setData] = useState<Loaded | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [highlightId, setHighlightId] = useState<number | null>(null);
  const [atBottom, setAtBottom] = useState(true);
  const [scrolling, setScrolling] = useState(false);
  const [lightbox, setLightbox] = useState<Message | null>(null);
  const viewerCtx = useMemo(() => ({ scrolling, openMedia: setLightbox }), [scrolling]);
  const loading = useRef(false);
  const virtuoso = useRef<VirtuosoHandle>(null);

  // Nova âncora vinda de fora com a conversa já aberta (o Viewer remonta este
  // componente ao trocar de conversa, então a primeira âncora vem do estado inicial).
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
          const r = await viewerApi.around(store, chat.id, anchor, PAGE);
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
          const msgs = await viewerApi.messages(store, chat.id, "before", null, PAGE);
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
    // `seq` muda a cada navegação (inclusive para a mesma âncora).
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [seq]);

  // O destaque some depois de um tempo.
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
      const older = await viewerApi.messages(store, chat.id, "before", { sentAt: first.sentAt, id: first.id }, PAGE);
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
  }, [data, store, chat.id]);

  const loadNewer = useCallback(async () => {
    if (!data?.hasNewer || loading.current) return;
    loading.current = true;
    try {
      const last = data.messages[data.messages.length - 1];
      const newer = await viewerApi.messages(store, chat.id, "after", { sentAt: last.sentAt, id: last.id }, PAGE);
      setData((d) => d && { ...d, messages: [...d.messages, ...newer], hasNewer: newer.length === PAGE });
    } finally {
      loading.current = false;
    }
  }, [data, store, chat.id]);

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
      ? "Grupo"
      : chat.kind === "broadcast"
        ? "Lista de transmissão"
        : chat.kind === "channel"
          ? "Canal"
          : chat.phone && chat.title
            ? formatPhone(chat.phone)
            : null;

  const components = useMemo(
    () => ({
      Header: () =>
        data?.hasOlder ? (
          <div className="list-edge">Carregando mensagens antigas…</div>
        ) : (
          <div className="list-edge">
            <span className="e2e-note">🔒 Histórico importado de um backup. Fica só neste computador, cifrado.</span>
          </div>
        ),
      Footer: () => (data?.hasNewer ? <div className="list-edge">Carregando…</div> : <div style={{ height: 12 }} />),
    }),
    [data?.hasOlder, data?.hasNewer],
  );

  return (
    <section className="chat-pane">
      <header className="chat-header">
        <Avatar chat={chat} />
        <div className="chat-header-text">
          <strong>{chatTitle(chat)}</strong>
          <span>
            {[subtitle, `${formatCount(chat.messageCount)} mensagens`].filter(Boolean).join(" · ")}
          </span>
        </div>
        <label className="icon-btn" title="Ir para uma data">
          📅
          <input type="date" className="date-input" onChange={(e) => onPickDate(e.target.value)} />
        </label>
        <button className="icon-btn" title="Buscar nesta conversa" onClick={onSearchInChat}>
          🔍
        </button>
      </header>

      <ViewerContext.Provider value={viewerCtx}>
      <div className="messages-wrap">
        {error && <div className="result error pane-error">{error}</div>}
        {data && data.messages.length === 0 && <div className="list-edge">Nenhuma mensagem.</div>}
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
                store={store}
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
          <button className="to-bottom" onClick={goToEnd} title="Ir para a mensagem mais recente">
            ⌄
          </button>
        )}
      </div>
      </ViewerContext.Provider>
      {lightbox && <Lightbox store={store} m={lightbox} onClose={() => setLightbox(null)} />}
    </section>
  );
}
