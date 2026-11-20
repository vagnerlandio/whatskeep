import { Icon, useLang } from "@wk/viewer";
import { FormEvent, useState } from "react";
import { api, errorOf, formatSize } from "../api";
import ConfirmDelete from "./ConfirmDelete";
import { useAppT } from "../i18n/app";
import { useErrorText } from "../i18n/errors";

type Props = {
  reason: string | null;
  onUnlocked: () => void;
  /** Saving a backup copy to a folder does not need the vault. */
  onDownloadOnly: () => void;
  /** The vault was deleted (forgotten password). */
  onDeleted: () => void;
};

export default function Unlock({ reason, onUnlocked, onDownloadOnly, onDeleted }: Props) {
  const t = useAppT();
  const { lang } = useLang();
  // Forgotten password: the delete dialog, with the vault size once known.
  const [forgot, setForgot] = useState<{ size: number | null } | null>(null);

  async function openForgot() {
    setForgot({ size: null });
    try {
      const size = await api.usage();
      setForgot((f) => f && { size });
    } catch {
      // The size is only informative.
    }
  }
  const errorText = useErrorText();
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);
  // Raw error (or wrong password): the text follows language changes.
  const [error, setError] = useState<{ wrong: true } | { e: unknown } | null>(null);

  async function submit(e: FormEvent) {
    e.preventDefault();
    if (!password || busy) return;
    setBusy(true);
    setError(null);
    try {
      await api.unlock(password);
      setPassword("");
      onUnlocked();
    } catch (err) {
      setError(errorOf(err).code === "wrong_password" ? { wrong: true } : { e: err });
      setPassword("");
      setBusy(false);
    }
  }

  return (
    <>
      <form className="card narrow" onSubmit={submit}>
        <div className="lock-icon">
          <Icon name="lockKeyhole" size={40} strokeWidth={1.5} />
        </div>
        <h1>{t.unlockTitle}</h1>
        {reason && <p className="muted">{reason}</p>}
        <label>
          {t.vaultPassword}
          <input
            type="password"
            autoFocus
            autoComplete="current-password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            disabled={busy}
          />
        </label>
        {error && <div className="result error">{"wrong" in error ? t.wrongPassword : errorText(error.e)}</div>}
        <button className="primary" type="submit" disabled={!password || busy}>
          {busy ? t.opening : t.open}
        </button>
        <button type="button" className="link center" onClick={onDownloadOnly}>
          {t.downloadWithoutVault}
        </button>
        <button type="button" className="link center muted" onClick={openForgot}>
          {t.forgotPassword}
        </button>
      </form>
      {forgot && (
        <ConfirmDelete
          title={t.deleteVaultTitle}
          onClose={() => setForgot(null)}
          onConfirm={async () => {
            await api.deleteVault();
            setForgot(null);
            onDeleted();
          }}
        >
          <p>{t.deleteVaultForgot}</p>
          <p>{t.deleteVaultWhat(forgot.size === null ? "…" : formatSize(forgot.size, lang))}</p>
          <p className="muted small">{t.deleteVaultKept}</p>
        </ConfirmDelete>
      )}
    </>
  );
}
