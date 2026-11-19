import { Icon } from "@wa/viewer";
import { FormEvent, useState } from "react";
import { api, errorOf } from "../api";
import { useAppT } from "../i18n/app";
import { useErrorText } from "../i18n/errors";

export default function Unlock({ reason, onUnlocked }: { reason: string | null; onUnlocked: () => void }) {
  const t = useAppT();
  const errorText = useErrorText();
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);
  // Erro bruto (ou senha errada): o texto acompanha a troca de idioma.
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
    </form>
  );
}
