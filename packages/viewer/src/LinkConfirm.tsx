import { useEffect, useState } from "react";
import { useSource } from "./source";
import { errorOf } from "./types";

/** Host em destaque e URL completa: o usuário vê para onde vai antes de sair. */
export default function LinkConfirm({ url, onClose }: { url: string; onClose: () => void }) {
  const source = useSource();
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
        <h2>Abrir link externo?</h2>
        {valid ? (
          <>
            <p className="muted small">O link vai abrir no navegador, fora deste histórico. Ele veio de uma mensagem: confira o endereço.</p>
            <p className="link-host">{host}</p>
            <p className="link-full">{url}</p>
          </>
        ) : (
          <p className="result error">Este link não pode ser aberto.</p>
        )}
        {error && <div className="result error">{error}</div>}
        <div className="row-buttons">
          <button onClick={onClose}>Cancelar</button>
          {valid && (
            <button className="primary" onClick={go}>
              Abrir
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
