import { memo, useState } from "react";
import { formatSize, Message, SourceApp, thumbUrl } from "../api";
import { formatDay, formatDuration, formatTime, personLabel, senderColor, WaText } from "./format";

/** Textos maiores que isso começam recolhidos ("Ler mais"), como no WhatsApp. */
const COLLAPSE_AT = 1500;
const MEDIA_MAX = 330;

type Props = {
  store: SourceApp;
  message: Message;
  /** Mensagem anterior na lista (para separador de dia e agrupamento). */
  prev: Message | undefined;
  isGroup: boolean;
  highlighted: boolean;
  onJump: (messageId: number) => void;
};

function MessageRow({ store, message: m, prev, isGroup, highlighted, onJump }: Props) {
  const newDay = !prev || new Date(prev.sentAt).toDateString() !== new Date(m.sentAt).toDateString();
  const sameAuthor =
    !newDay && prev && prev.kind !== "system" && prev.fromMe === m.fromMe && prev.sender?.jid === m.sender?.jid;

  return (
    <div className={`row ${sameAuthor ? "cont" : ""}`}>
      {newDay && (
        <div className="day">
          <span>{formatDay(m.sentAt)}</span>
        </div>
      )}
      {m.kind === "system" ? (
        <div className="system-msg">
          <span>{m.text ?? "Mensagem do sistema"}</span>
        </div>
      ) : (
        <Bubble store={store} m={m} showSender={isGroup && !m.fromMe && !sameAuthor} highlighted={highlighted} onJump={onJump} />
      )}
    </div>
  );
}

export default memo(MessageRow);

function Bubble({
  store,
  m,
  showSender,
  highlighted,
  onJump,
}: {
  store: SourceApp;
  m: Message;
  showSender: boolean;
  highlighted: boolean;
  onJump: (id: number) => void;
}) {
  const sticker = m.kind === "sticker";
  const cls = ["bubble", m.fromMe ? "out" : "in", sticker ? "sticker" : "", highlighted ? "flash" : ""].join(" ");

  return (
    <div className={`bubble-line ${m.fromMe ? "out" : "in"}`}>
      <div className={cls}>
        {showSender && m.sender && (
          <div className="sender" style={{ color: senderColor(m.sender.jid) }}>
            {personLabel(m.sender)}
          </div>
        )}
        {m.quoted && (
          <button
            className="quote"
            disabled={m.quoted.messageId === null}
            onClick={() => m.quoted?.messageId != null && onJump(m.quoted.messageId)}
          >
            <span className="quote-author">{m.quoted.fromMe ? "Você" : personLabel(m.quoted.sender)}</span>
            <span className="quote-text">{m.quoted.text ?? "Mídia"}</span>
          </button>
        )}
        <Body store={store} m={m} />
        <span className="meta">
          {m.starred && <span title="Favoritada">★</span>}
          {m.edited && <span>Editada</span>}
          <span>{formatTime(m.sentAt)}</span>
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

function Body({ store, m }: { store: SourceApp; m: Message }) {
  switch (m.kind) {
    case "deleted":
      return <p className="text muted-italic">🚫 {m.fromMe ? "Você apagou esta mensagem" : "Esta mensagem foi apagada"}</p>;
    case "image":
    case "video":
    case "gif":
    case "sticker":
      return (
        <>
          <Visual store={store} m={m} />
          {m.text && m.kind !== "sticker" && <Text text={m.text} />}
        </>
      );
    case "voice":
    case "audio":
      return (
        <>
          <div className="audio">
            <span className="audio-icon">{m.kind === "voice" ? "🎤" : "🎵"}</span>
            <span className="audio-bar" />
            <span className="small">{formatDuration(m.media?.durationS ?? null)}</span>
          </div>
          {m.media?.transcription && <p className="transcription">“{m.media.transcription}”</p>}
          <NotImported />
        </>
      );
    case "document":
      return (
        <>
          <div className="doc">
            <span className="doc-icon">📄</span>
            <span className="doc-name">
              {m.media?.fileName ?? "Documento"}
              <small>
                {[m.media?.mime?.split("/")[1]?.toUpperCase(), m.media?.size ? formatSize(m.media.size) : null]
                  .filter(Boolean)
                  .join(" · ")}
              </small>
            </span>
          </div>
          {m.text && <Text text={m.text} />}
          <NotImported />
        </>
      );
    case "contact":
      return (
        <div className="contact-cards">
          {(m.contacts.length ? m.contacts : [{ name: m.text, phones: [] }]).map((c, i) => (
            <div className="contact-card" key={i}>
              <span className="avatar small">👤</span>
              <span>
                {c.name ?? "Contato"}
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
          <span className="place-icon">📍</span>
          <span>
            {m.location?.name ?? (m.kind === "live_location" ? "Localização em tempo real" : "Localização")}
            {m.location?.address && <small>{m.location.address}</small>}
            {m.location && (
              <small className="mono">
                {m.location.latitude.toFixed(5)}, {m.location.longitude.toFixed(5)}
              </small>
            )}
          </span>
        </div>
      );
    case "unknown":
      return (
        <>
          <p className="text muted-italic">⚠️ Este tipo de mensagem ainda não é suportado.</p>
          {m.text && <Text text={m.text} />}
        </>
      );
    default:
      return m.text ? <Text text={m.text} /> : null;
  }
}

function Text({ text }: { text: string }) {
  const [expanded, setExpanded] = useState(false);
  const long = text.length > COLLAPSE_AT;
  return (
    <p className="text">
      <WaText text={long && !expanded ? text.slice(0, COLLAPSE_AT) + "…" : text} />
      {long && !expanded && (
        <button className="link" onClick={() => setExpanded(true)}>
          Ler mais
        </button>
      )}
    </p>
  );
}

const NotImported = () => <span className="not-imported">Arquivo não incluído no histórico</span>;

/** Foto/vídeo/figurinha com o espaço reservado pelas dimensões gravadas, para
 * a lista não "pular" quando a miniatura carrega. */
function Visual({ store, m }: { store: SourceApp; m: Message }) {
  const sticker = m.kind === "sticker";
  let w = sticker ? 140 : 240;
  let h = sticker ? 140 : 180;
  const mw = m.media?.width;
  const mh = m.media?.height;
  if (!sticker && mw && mh) {
    const scale = Math.min(MEDIA_MAX / mw, MEDIA_MAX / mh);
    w = Math.max(120, Math.round(mw * scale));
    h = Math.max(80, Math.round(mh * scale));
  }
  const label = m.kind === "video" ? "🎥 " + formatDuration(m.media?.durationS ?? null) : m.kind === "gif" ? "GIF" : null;

  return (
    <div className="visual" style={{ width: w, height: h }}>
      {m.media?.hasThumbnail ? (
        <img src={thumbUrl(store, m.id)} alt="" loading="lazy" decoding="async" draggable={false} />
      ) : (
        <span className="visual-placeholder">{sticker ? "💟" : m.kind === "image" ? "📷" : "🎥"}</span>
      )}
      {label && <span className="visual-label">{label}</span>}
      {!sticker && <span className="visual-note">Prévia · arquivo não incluído</span>}
    </div>
  );
}
