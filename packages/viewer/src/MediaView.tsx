import { MouseEvent, useEffect, useState } from "react";
import { useViewer } from "./context";
import { formatDuration } from "./format";
import { useSource, useUrl } from "./source";
import { errorOf, Message } from "./types";
import { Icon } from "./icons";
import { useViewerT } from "./messages";

const MEDIA_MAX = 330;
/** Espera depois que a rolagem para antes de decodificar a imagem completa. */
const FULL_DELAY_MS = 150;

/** Foto/vídeo/GIF/figurinha. O espaço é reservado pelas dimensões gravadas,
 * para a lista não "pular". A miniatura aparece na hora; o arquivo completo
 * (se disponível) só carrega quando a rolagem para. */
export function Visual({ m }: { m: Message }) {
  const source = useSource();
  const { scrolling, openMedia } = useViewer();
  const t = useViewerT();
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

  const thumb = useUrl(() => (m.media?.hasThumbnail ? source.thumbUrl(m) : null), [source, m.id]);
  const fullUrl = useUrl(() => (full ? source.mediaUrl(m) : null), [source, m.id, full]);

  let w = sticker ? 140 : 240;
  let h = sticker ? 140 : 180;
  const mw = m.media?.width;
  const mh = m.media?.height;
  if (!sticker && mw && mh) {
    const scale = Math.min(MEDIA_MAX / mw, MEDIA_MAX / mh);
    w = Math.max(120, Math.round(mw * scale));
    h = Math.max(80, Math.round(mh * scale));
  }
  const label =
    m.kind === "video" ? (
      <>
        <Icon name="video" size={14} />
        {formatDuration(m.media?.durationS ?? null)}
      </>
    ) : animated ? (
      "GIF"
    ) : null;

  return (
    <div
      className={`visual ${available ? "clickable" : ""}`}
      style={{ width: w, height: h }}
      onClick={available ? () => openMedia(m) : undefined}
    >
      {thumb ? (
        <img src={thumb} alt="" loading="lazy" decoding="async" draggable={false} />
      ) : (
        !fullLoaded && (
          <span className="visual-placeholder">
            <Icon name={sticker ? "sticker" : m.kind === "image" ? "camera" : m.kind === "gif" ? "gif" : "video"} size={40} strokeWidth={1.25} />
          </span>
        )
      )}
      {fullUrl &&
        (animated ? (
          <video
            className={`full ${fullLoaded ? "loaded" : ""}`}
            src={fullUrl}
            autoPlay
            loop
            muted
            playsInline
            onLoadedData={() => setFullLoaded(true)}
          />
        ) : (
          <img
            className={`full ${fullLoaded ? "loaded" : ""}`}
            src={fullUrl}
            alt=""
            decoding="async"
            draggable={false}
            onLoad={() => setFullLoaded(true)}
          />
        ))}
      {m.kind === "video" && available && (
        <span className="play-overlay">
          <Icon name="play" size={28} fill />
        </span>
      )}
      {label && <span className="visual-label">{label}</span>}
      {!sticker && !available && <span className="visual-note">{t.previewNotIncluded}</span>}
    </div>
  );
}

export function AudioView({ m }: { m: Message }) {
  const source = useSource();
  const [play, setPlay] = useState(false);
  const t = useViewerT();
  // Nada é carregado (nem decifrado) até o usuário pedir.
  const url = useUrl(() => (play ? source.mediaUrl(m) : null), [source, m.id, play]);
  const icon = <Icon name={m.kind === "voice" ? "mic" : "headphones"} size={22} />;
  if (!m.media?.available) {
    return (
      <>
        <div className="audio">
          <span className="audio-icon">{icon}</span>
          <span className="audio-bar" />
          <span className="small">{formatDuration(m.media?.durationS ?? null)}</span>
        </div>
        <span className="not-imported">{t.fileNotIncluded}</span>
      </>
    );
  }
  return (
    <div className="audio">
      <span className="audio-icon">{icon}</span>
      {play && url ? (
        <audio controls autoPlay src={url} />
      ) : (
        <>
          <button className="icon-btn small" title={t.play} onClick={() => setPlay(true)}>
            <Icon name="play" size={20} fill />
          </button>
          <span className="audio-bar" />
          <span className="small">{play ? (source.mediaHint?.() ?? t.loading) : formatDuration(m.media?.durationS ?? null)}</span>
        </>
      )}
      {source.saveMedia && <SaveButton m={m} compact />}
    </div>
  );
}

export function SaveButton({ m, compact }: { m: Message; compact?: boolean }) {
  const source = useSource();
  const [state, setState] = useState<"idle" | "saving" | "saved" | string>("idle");
  const t = useViewerT();
  if (!source.saveMedia) return null;
  const save = async (e: MouseEvent) => {
    e.stopPropagation();
    setState("saving");
    try {
      setState((await source.saveMedia!(m)) ? "saved" : "idle");
    } catch (err) {
      setState(errorOf(err).message);
    }
  };
  const label =
    state === "saving" ? (
      t.saving
    ) : state === "saved" ? (
      <>
        <Icon name="check" size={16} /> {t.saved}
      </>
    ) : compact ? (
      <Icon name="download" size={18} />
    ) : (
      <>
        <Icon name="download" size={16} /> {t.save}
      </>
    );
  return (
    <>
      <button className={compact ? "icon-btn small" : "save-btn"} title={t.saveCopy} onClick={save}>
        {label}
      </button>
      {!["idle", "saving", "saved"].includes(state) && <span className="hint bad">{state}</span>}
    </>
  );
}

/** Mídia em tela cheia. Esc ou clique fora fecha. */
export function Lightbox({ m, onClose }: { m: Message; onClose: () => void }) {
  const source = useSource();
  const url = useUrl(() => source.mediaUrl(m), [source, m.id]);
  const [waited, setWaited] = useState(false);
  const t = useViewerT();
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    const t = setTimeout(() => setWaited(true), 400);
    return () => {
      window.removeEventListener("keydown", onKey);
      clearTimeout(t);
    };
  }, [onClose]);
  const video = m.kind === "video" || m.kind === "gif";
  return (
    <div className="lightbox" onClick={onClose} role="dialog" aria-modal>
      <div className="lightbox-bar" onClick={(e) => e.stopPropagation()}>
        <SaveButton m={m} />
        <button className="icon-btn" title={t.close} onClick={onClose}>
          <Icon name="close" size={24} />
        </button>
      </div>
      <div className="lightbox-body" onClick={(e) => e.stopPropagation()}>
        {url ? (
          video ? (
            <video src={url} controls autoPlay loop={m.kind === "gif"} playsInline />
          ) : (
            <img src={url} alt="" />
          )
        ) : (
          waited && <p className="lightbox-caption">{source.mediaHint?.() ?? t.loading}</p>
        )}
        {m.text && <p className="lightbox-caption">{m.text}</p>}
      </div>
    </div>
  );
}
