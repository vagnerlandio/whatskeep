import { useEffect, useState } from "react";
import { useSource } from "./source";
import { errorOf } from "./types";
import { useViewerT } from "./messages";

/** Host em destaque e URL completa: o usuário vê para onde vai antes de sair. */
export default function LinkConfirm({ url, onClose }: { url: string; onClose: () => void }) {
  const source = useSource();
  const t = useViewerT();
  const [error, setError] = useState<string | null>(null);
  let host = "";
  let valid = false;
  try {
    const u = new URL(url);
    host = u.hostname;
    valid = (u.protocol === "http:" || u.protocol === "https:") && !!host && !u.username && !u.password;
  } catch {
    valid = false;
  }

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  async function go() {
    try {
      if (source.openLink) await source.openLink(url);
      else window.open(url, "_blank", "noopener,noreferrer");
      onClose();
    } catch (e) {
      setError(errorOf(e).message);
    }
  }

  return (
    <div className="modal-backdrop" onClick={onClose} role="dialog" aria-modal>
      <div className="card narrow" onClick={(e) => e.stopPropagation()}>
        <h2>{t.linkTitle}</h2>
        {valid ? (
          <>
            <p className="muted small">{t.linkExplain}</p>
            <p className="link-host">{host}</p>
            <p className="link-full">{url}</p>
          </>
        ) : (
          <p className="result error">{t.linkInvalid}</p>
        )}
        {error && <div className="result error">{error}</div>}
        <div className="row-buttons">
          <button onClick={onClose}>{t.cancel}</button>
          {valid && (
            <button className="primary" onClick={go}>
              {t.open}
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
