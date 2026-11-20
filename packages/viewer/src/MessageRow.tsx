import { CSSProperties, memo, useState } from "react";
import { Card, formatSize, Message } from "./types";
import { formatDateTime, formatDay, formatTime, personLabel, senderColor, WaText } from "./format";
import { AudioView, SaveButton, Visual } from "./MediaView";
import { Icon } from "./icons";
import { useLang } from "./i18n";
import { useViewerT } from "./messages";

/** Texts longer than this start collapsed ("Read more"), like in WhatsApp. */
const COLLAPSE_AT = 1500;

type Props = {
  message: Message;
  /** Previous message in the list (for the day separator and grouping). */
  prev: Message | undefined;
  isGroup: boolean;
  highlighted: boolean;
  onJump: (messageId: number) => void;
};

function MessageRow({ message: m, prev, isGroup, highlighted, onJump }: Props) {
  // Hooks in here: memo doesn't prevent re-rendering when the language changes.
  const t = useViewerT();
  const { lang } = useLang();
  const newDay = !prev || new Date(prev.sentAt).toDateString() !== new Date(m.sentAt).toDateString();
  const sameAuthor =
    !newDay && prev && prev.kind !== "system" && prev.fromMe === m.fromMe && prev.sender?.jid === m.sender?.jid;

  return (
    <div className={`row ${sameAuthor ? "cont" : ""}`}>
      {newDay && (
        <div className="day">
          <span>{formatDay(m.sentAt, lang)}</span>
        </div>
      )}
      {m.kind === "system" ? (
        <div className="system-msg">
          <span>{m.text ?? noticeText(m.card, t) ?? t.kinds.system}</span>
        </div>
      ) : (
        <Bubble
          m={m}
          first={!sameAuthor}
          showSender={isGroup && !m.fromMe && !sameAuthor}
          highlighted={highlighted}
          onJump={onJump}
        />
      )}
    </div>
  );
}

export default memo(MessageRow);

function Bubble({
  m,
  first,
  showSender,
  highlighted,
  onJump,
}: {
  m: Message;
  /** First of a run from the same author: gets the "tail". */
  first: boolean;
  showSender: boolean;
  highlighted: boolean;
  onJump: (id: number) => void;
}) {
  const t = useViewerT();
  const { lang } = useLang();
  const sticker = m.kind === "sticker";
  const cls = ["bubble", m.fromMe ? "out" : "in", first && !sticker ? "tail" : "", sticker ? "sticker" : "", highlighted ? "flash" : ""]
    .filter(Boolean)
    .join(" ");

  return (
    <div className={`bubble-line ${m.fromMe ? "out" : "in"}`}>
      <div className={cls}>
        {showSender && m.sender && (
          <div className="sender" style={{ color: senderColor(m.sender.jid) }}>
            {personLabel(m.sender, lang)}
          </div>
        )}
        {m.quoted && (
          <button
            className="quote"
            style={{ "--quote-color": m.quoted.fromMe ? "#06cf9c" : senderColor(m.quoted.sender?.jid ?? "") } as CSSProperties}
            disabled={m.quoted.messageId === null}
            onClick={() => m.quoted?.messageId != null && onJump(m.quoted.messageId)}
          >
            <span className="quote-author">{m.quoted.fromMe ? t.you : personLabel(m.quoted.sender, lang)}</span>
            <span className="quote-text">{m.quoted.text ?? t.media}</span>
          </button>
        )}
        <Body m={m} />
        {m.card?.type === "buttons" && <Buttons card={m.card} />}
        <span className="meta">
          {m.starred && <Icon name="star" size={11} fill title={t.starred} />}
          {m.edited && <span>{t.edited}</span>}
          <span>{formatTime(m.sentAt, lang)}</span>
        </span>
      </div>
      {m.reactions.length > 0 && (
        <div className="reactions">
          {m.reactions.map((r) => (
            <span key={r.emoji} className={r.mine ? "mine" : ""}>
              {r.emoji}
              {r.count > 1 && <small>{r.count}</small>}
            </span>
          ))}
        </div>
      )}
    </div>
  );
}

function Body({ m }: { m: Message }) {
  const t = useViewerT();
  const { lang } = useLang();
  switch (m.kind) {
    case "deleted":
      return (
        <p className="text muted-italic with-icon">
          <Icon name="cancel" size={16} />
          {m.fromMe ? t.youDeleted : t.wasDeleted}
        </p>
      );
    case "image":
    case "video":
    case "gif":
    case "sticker":
      return (
        <>
          <Visual m={m} />
          {m.text && m.kind !== "sticker" && <Text text={m.text} />}
        </>
      );
    case "voice":
    case "audio":
      return (
        <>
          <AudioView m={m} />
          {m.media?.transcription && <p className="transcription">“{m.media.transcription}”</p>}
        </>
      );
    case "document":
      return (
        <>
          <div className="doc">
            <span className="doc-icon">
              <Icon name="document" size={28} strokeWidth={1.5} />
            </span>
            <span className="doc-name">
              {m.media?.fileName ?? t.document}
              <small>
                {[m.media?.mime?.split("/")[1]?.toUpperCase(), m.media?.size ? formatSize(m.media.size, lang) : null]
                  .filter(Boolean)
                  .join(" · ")}
              </small>
            </span>
          </div>
          {m.text && <Text text={m.text} />}
          {m.media?.available ? <SaveButton m={m} /> : <NotImported />}
        </>
      );
    case "contact":
      return (
        <div className="contact-cards">
          {(m.contacts.length ? m.contacts : [{ name: m.text, phones: [] }]).map((c, i) => (
            <div className="contact-card" key={i}>
              <span className="avatar small">
                <Icon name="account" size={20} />
              </span>
              <span>
                {c.name ?? t.contact}
                {c.phones.map((p) => (
                  <small key={p}>{p}</small>
                ))}
              </span>
            </div>
          ))}
        </div>
      );
    case "location":
    case "live_location":
      return (
        <div className="place">
          <span className="place-icon">
            <Icon name="location" size={24} />
          </span>
          <span>
            {m.location?.name ?? (m.kind === "live_location" ? t.liveLocation : t.location)}
            {m.location?.address && <small>{m.location.address}</small>}
            {m.location && (
              <small className="mono">
                {m.location.latitude.toFixed(5)}, {m.location.longitude.toFixed(5)}
              </small>
            )}
          </span>
        </div>
      );
    case "waiting":
      return (
        <p className="text muted-italic with-icon">
          <Icon name="waiting" size={16} />
          {t.waiting}
        </p>
      );
    case "status_mention":
      return (
        <p className="text muted-italic with-icon">
          <Icon name="mention" size={16} />
          {t.statusMention}
        </p>
      );
    case "poll":
      return m.card?.type === "poll" ? <Poll question={m.text} card={m.card} /> : m.text ? <Text text={m.text} /> : null;
    case "event":
      return <Event m={m} card={m.card?.type === "event" ? m.card : null} />;
    case "product":
      return (
        <>
          {m.card?.type === "product" && <Product card={m.card} />}
          {m.text && <Text text={m.text} />}
        </>
      );
    case "invite":
      return (
        <>
          <div className="msg-card">
            <span className="msg-card-icon">
              <Icon name="group" size={22} />
            </span>
            <span className="msg-card-body">
              <strong>{(m.card?.type === "invite" && m.card.group) || t.groupInvite}</strong>
              <small>{t.groupInvite}</small>
            </span>
          </div>
          {m.text && <Text text={m.text} />}
        </>
      );
    case "unknown":
      return (
        <>
          <p className="text muted-italic with-icon">
            <Icon name="alert" size={16} />
            {t.unsupported}
          </p>
          {m.text && <Text text={m.text} />}
        </>
      );
    default:
      return m.text ? <Text text={m.text} /> : null;
  }
}

type ViewerT = ReturnType<typeof useViewerT>;

/** Text of a notice that only has a card (disappearing messages, privacy...). */
function noticeText(card: Card | null | undefined, t: ViewerT): string | null {
  switch (card?.type) {
    case "disappearing":
      return card.seconds > 0 ? t.disappearingOn(durationText(card.seconds, t)) : t.disappearingOff;
    case "chatPrivacy":
      return card.enabled ? t.chatPrivacyOn : t.chatPrivacyOff;
    case "sharedHistory":
      return t.sharedHistory(card.messages);
    default:
      return null;
  }
}

/** 86400 → "24 hours", 604800 → "7 days" (the durations WhatsApp offers). */
function durationText(seconds: number, t: ViewerT): string {
  const hours = Math.round(seconds / 3600);
  return hours <= 24 || hours % 24 !== 0 ? t.hours(Math.max(1, hours)) : t.days(hours / 24);
}

function Poll({ question, card }: { question: string | null; card: Extract<Card, { type: "poll" }> }) {
  const t = useViewerT();
  const total = card.options.reduce((n, o) => n + o.votes, 0);
  return (
    <div className="poll">
      <span className="poll-label">
        <Icon name="poll" size={14} />
        {t.poll}
      </span>
      {question && <p className="poll-question">{question}</p>}
      <small className="muted">{card.multiple ? t.pollMultiple : t.pollSingle}</small>
      <ul className="poll-options">
        {card.options.map((o, i) => (
          <li key={i}>
            <span className="poll-option">
              <span>{o.name}</span>
              <span className="poll-votes">{o.votes}</span>
            </span>
            <span className="poll-bar">
              <span style={{ width: total ? `${(o.votes / total) * 100}%` : 0 }} />
            </span>
          </li>
        ))}
      </ul>
      <small className="muted">{t.votes(total)}</small>
    </div>
  );
}

function Event({ m, card }: { m: Message; card: Extract<Card, { type: "event" }> | null }) {
  const t = useViewerT();
  const { lang } = useLang();
  const name = card?.name ?? m.text;
  const when = card?.startMs
    ? [formatDateTime(card.startMs, lang), card.endMs ? formatDateTime(card.endMs, lang) : null].filter(Boolean).join(" – ")
    : null;
  return (
    <>
      <div className={card?.canceled ? "msg-card canceled" : "msg-card"}>
        <span className="msg-card-icon">
          <Icon name="calendar" size={22} />
        </span>
        <span className="msg-card-body">
          <small>{card?.canceled ? t.eventCanceled : card?.call ? t.eventCall : t.event}</small>
          {name && <strong>{name}</strong>}
          {when && <small>{when}</small>}
          {card?.place && (
            <small className="with-icon">
              <Icon name="location" size={13} />
              {card.place}
            </small>
          )}
        </span>
      </div>
      {card?.description && <Text text={card.description} />}
      {m.text && m.text !== name && <Text text={m.text} />}
    </>
  );
}

function Product({ card }: { card: Extract<Card, { type: "product" }> }) {
  const t = useViewerT();
  const { lang } = useLang();
  let price: string | null = null;
  if (card.amount1000 != null) {
    const value = card.amount1000 / 1000;
    try {
      price = card.currency
        ? new Intl.NumberFormat(lang, { style: "currency", currency: card.currency }).format(value)
        : new Intl.NumberFormat(lang).format(value);
    } catch {
      price = new Intl.NumberFormat(lang).format(value);
    }
  }
  return (
    <div className="msg-card">
      <span className="msg-card-icon">
        <Icon name="product" size={22} />
      </span>
      <span className="msg-card-body">
        <strong>{card.title ?? t.product}</strong>
        {card.description && <small>{card.description}</small>}
        {price && <small className="price">{price}</small>}
      </span>
    </div>
  );
}

/** Footer and buttons of a business message. The buttons are shown, not used. */
function Buttons({ card }: { card: Extract<Card, { type: "buttons" }> }) {
  return (
    <>
      {card.footer && <p className="msg-footer">{card.footer}</p>}
      {card.buttons.length > 0 && (
        <div className="msg-buttons">
          {card.buttons.map((b, i) => (
            <span key={i}>{b}</span>
          ))}
        </div>
      )}
    </>
  );
}

function Text({ text }: { text: string }) {
  const t = useViewerT();
  const [expanded, setExpanded] = useState(false);
  const long = text.length > COLLAPSE_AT;
  return (
    <p className="text">
      <WaText text={long && !expanded ? text.slice(0, COLLAPSE_AT) + "…" : text} />
      {long && !expanded && (
        <button className="link" onClick={() => setExpanded(true)}>
          {t.readMore}
        </button>
      )}
    </p>
  );
}

function NotImported() {
  const t = useViewerT();
  return <span className="not-imported">{t.fileNotIncluded}</span>;
}
