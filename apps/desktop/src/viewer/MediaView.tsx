import { useEffect, useState } from "react";
import { errorOf, mediaApi, mediaUrl, Message, SourceApp, thumbUrl } from "../api";
import { formatDuration } from "./format";
import { useViewer } from "./context";

const MEDIA_MAX = 330;
/** Espera depois que a rolagem para antes de decodificar a imagem completa. */
const FULL_DELAY_MS = 150;

/** Foto/vídeo/GIF/figurinha. O espaço é reservado pelas dimensões gravadas,
 * para a lista não "pular". A miniatura aparece na hora; o arquivo completo
 * (se estiver no cofre) só carrega quando a rolagem para. */
export function Visual({ store, m }: { store: SourceApp; m: Message }) {
  const { scrolling, openMedia } = useViewer();
  const sticker = m.kind === "sticker";
  const available = !!m.media?.available;
  const animated = m.kind === "gif";
  const [full, setFull] = useState(false);
  const [fullLoaded, setFullLoaded] = useState(false);

  useEffect(() => {
    if (!available || full || scrolling || m.kind === "video") return;
    const t = setTimeout(() => setFull(true), FULL_DELAY_MS);
    return () => clearTimeout(t);
  }, [available, full, scrolling, m.kind]);

  let w = sticker ? 140 : 240;
  let h = sticker ? 140 : 180;
  const mw = m.media?.width;
  const mh = m.media?.height;
  if (!sticker && mw && mh) {
    const scale = Math.min(MEDIA_MAX / mw, MEDIA_MAX / mh);
    w = Math.max(120, Math.round(mw * scale));
    h = Math.max(80, Math.round(mh * scale));
  }
  const label = m.kind === "video" ? "▶ " + formatDuration(m.media?.durationS ?? null) : animated ? "GIF" : null;

  return (
    <div
      className={`visual ${available ? "clickable" : ""}`}
      style={{ width: w, height: h }}
      onClick={available ? () => openMedia(m) : undefined}
    >
      {m.media?.hasThumbnail ? (
        <img src={thumbUrl(store, m.id)} alt="" loading="lazy" decoding="async" draggable={false} />
      ) : (
        !fullLoaded && <span className="visual-placeholder">{sticker ? "💟" : m.kind === "image" ? "📷" : "🎥"}</span>
      )}
      {full &&
        (animated ? (
          <video
            className={`full ${fullLoaded ? "loaded" : ""}`}
            src={mediaUrl(store, m.id)}
            autoPlay
            loop
            muted
            playsInline
            onLoadedData={() => setFullLoaded(true)}
          />
        ) : (
          <img
            className={`full ${fullLoaded ? "loaded" : ""}`}
            src={mediaUrl(store, m.id)}
            alt=""
            decoding="async"
            draggable={false}
            onLoad={() => setFullLoaded(true)}
          />
        ))}
      {label && <span className="visual-label">{label}</span>}
      {!sticker && !available && <span className="visual-note">Prévia · arquivo não incluído</span>}
    </div>
  );
}

export function AudioView({ store, m }: { store: SourceApp; m: Message }) {
  if (!m.media?.available) {
    return (
      <>
        <div className="audio">
          <span className="audio-icon">{m.kind === "voice" ? "🎤" : "🎵"}</span>
          <span className="audio-bar" />
          <span className="small">{formatDuration(m.media?.durationS ?? null)}</span>
        </div>
        <span className="not-imported">Arquivo não incluído no histórico</span>
      </>
    );
  }
  return (
    <div className="audio">
      <span className="audio-icon">{m.kind === "voice" ? "🎤" : "🎵"}</span>
      {/* preload="none": nada é decifrado até o usuário apertar play. */}
      <audio controls preload="none" src={mediaUrl(store, m.id)} />
      <SaveButton store={store} m={m} compact />
    </div>
  );
}

export function SaveButton({ store, m, compact }: { store: SourceApp; m: Message; compact?: boolean }) {
  const [state, setState] = useState<"idle" | "saving" | "saved" | string>("idle");
  async function save(e: React.MouseEvent) {
    e.stopPropagation();
    setState("saving");
    try {
      setState((await mediaApi.save(store, m.id)) ? "saved" : "idle");
    } catch (err) {
      setState(errorOf(err).message);
    }
  }
  const label = state === "saving" ? "Salvando…" : state === "saved" ? "Salvo ✓" : compact ? "⤓" : "Salvar…";
  return (
    <>
      <button className={compact ? "icon-btn small" : "save-btn"} title="Salvar uma cópia" onClick={save}>
        {label}
      </button>
      {!["idle", "saving", "saved"].includes(state) && <span className="hint bad">{state}</span>}
    </>
  );
}

/** Mídia em tela cheia. Esc ou clique fora fecha. */
export function Lightbox({ store, m, onClose }: { store: SourceApp; m: Message; onClose: () => void }) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);
  const video = m.kind === "video" || m.kind === "gif";
  return (
    <div className="lightbox" onClick={onClose} role="dialog" aria-modal>
      <div className="lightbox-bar" onClick={(e) => e.stopPropagation()}>
        <SaveButton store={store} m={m} />
        <button className="icon-btn" title="Fechar" onClick={onClose}>
          ✕
        </button>
      </div>
      <div className="lightbox-body" onClick={(e) => e.stopPropagation()}>
        {video ? (
          <video src={mediaUrl(store, m.id)} controls autoPlay loop={m.kind === "gif"} playsInline />
        ) : (
          <img src={mediaUrl(store, m.id)} alt="" />
        )}
        {m.text && <p className="lightbox-caption">{m.text}</p>}
      </div>
    </div>
  );
}
