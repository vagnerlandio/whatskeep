import { FormEvent, ReactNode, useState } from "react";
import { useAppT } from "../i18n/app";
import { useErrorText } from "../i18n/errors";

type Props = {
  title: string;
  children: ReactNode;
  /** Deletes; a rejection shows the error and keeps the dialog open. */
  onConfirm: () => Promise<void>;
  onClose: () => void;
};

/** Destructive confirmation: the button only works after typing the confirmation word. */
export default function ConfirmDelete({ title, children, onConfirm, onClose }: Props) {
  const t = useAppT();
  const errorText = useErrorText();
  const [typed, setTyped] = useState("");
  const [busy, setBusy] = useState(false);
  // Raw error: the text follows language changes.
  const [error, setError] = useState<{ e: unknown } | null>(null);
  const ready = typed.trim().toUpperCase() === t.confirmWord && !busy;

  async function submit(e: FormEvent) {
    e.preventDefault();
    if (!ready) return;
    setBusy(true);
    setError(null);
    try {
      await onConfirm();
    } catch (err) {
      setError({ e: err });
      setBusy(false);
    }
  }

  return (
    <div className="modal-backdrop" role="dialog" aria-modal>
      <form className="card narrow" onSubmit={submit}>
        <h2>{title}</h2>
        {children}
        <label>
          {t.typeToConfirm(t.confirmWord)}
          <input
            autoFocus
            spellCheck={false}
            autoComplete="off"
            autoCapitalize="characters"
            value={typed}
            onChange={(e) => setTyped(e.target.value)}
            disabled={busy}
          />
        </label>
        {error && <div className="result error">{errorText(error.e)}</div>}
        <div className="row-buttons">
          <button type="button" onClick={onClose} disabled={busy}>
            {t.cancel}
          </button>
          <button type="submit" className="danger" disabled={!ready}>
            {busy ? t.deleting : t.deleteConfirm}
          </button>
        </div>
      </form>
    </div>
  );
}
