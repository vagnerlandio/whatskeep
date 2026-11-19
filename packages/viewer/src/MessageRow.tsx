import { CSSProperties, memo, useState } from "react";
import { formatSize, Message } from "./types";
import { formatDay, formatTime, personLabel, senderColor, WaText } from "./format";
import { AudioView, SaveButton, Visual } from "./MediaView";
import { Icon } from "./icons";

/** Textos maiores que isso começam recolhidos ("Ler mais"), como no WhatsApp. */
const COLLAPSE_AT = 1500;

type Props = {
  message: Message;
  /** Mensagem anterior na lista (para separador de dia e agrupamento). */
  prev: Message | undefined;
  isGroup: boolean;
  highlighted: boolean;
  onJump: (messageId: number) => void;
};

function MessageRow({ message: m, prev, isGroup, highlighted, onJump }: Props) {
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
  /** Primeira de uma sequência do mesmo autor: leva o "rabinho". */
  first: boolean;
  showSender: boolean;
  highlighted: boolean;
  onJump: (id: number) => void;
}) {
  const sticker = m.kind === "sticker";
  const cls = ["bubble", m.fromMe ? "out" : "in", first && !sticker ? "tail" : "", sticker ? "sticker" : "", highlighted ? "flash" : ""]
    .filter(Boolean)
    .join(" ");

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
            style={{ "--quote-color": m.quoted.fromMe ? "#06cf9c" : senderColor(m.quoted.sender?.jid ?? "") } as CSSProperties}
            disabled={m.quoted.messageId === null}
            onClick={() => m.quoted?.messageId != null && onJump(m.quoted.messageId)}
          >
            <span className="quote-author">{m.quoted.fromMe ? "Você" : personLabel(m.quoted.sender)}</span>
            <span className="quote-text">{m.quoted.text ?? "Mídia"}</span>
          </button>
        )}
        <Body m={m} />
        <span className="meta">
          {m.starred && <Icon name="star" size={11} fill title="Favoritada" />}
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

function Body({ m }: { m: Message }) {
  switch (m.kind) {
    case "deleted":
      return (
        <p className="text muted-italic with-icon">
          <Icon name="cancel" size={16} />
          {m.fromMe ? "Você apagou esta mensagem" : "Esta mensagem foi apagada"}
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
              {m.media?.fileName ?? "Documento"}
              <small>
                {[m.media?.mime?.split("/")[1]?.toUpperCase(), m.media?.size ? formatSize(m.media.size) : null]
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
          <span className="place-icon">
            <Icon name="location" size={24} />
          </span>
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
          <p className="text muted-italic with-icon">
            <Icon name="alert" size={16} />
            Este tipo de mensagem ainda não é suportado.
          </p>
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
