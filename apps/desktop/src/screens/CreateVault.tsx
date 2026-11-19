import { FormEvent, useState } from "react";
import { api, MIN_PASSWORD_CHARS } from "../api";
import { useAppT } from "../i18n/app";
import { useErrorText } from "../i18n/errors";

type Props = { autolockMinutes: number; onCreated: () => void };

export default function CreateVault({ autolockMinutes, onCreated }: Props) {
  const t = useAppT();
  const errorText = useErrorText();
  const [password, setPassword] = useState("");
  const [confirm, setConfirm] = useState("");
  const [busy, setBusy] = useState(false);
  // Raw error: the text follows language changes.
  const [error, setError] = useState<{ e: unknown } | null>(null);

  const length = [...password].length;
  const tooShort = length < MIN_PASSWORD_CHARS;
  const mismatch = confirm.length > 0 && confirm !== password;
  const canSubmit = !busy && !tooShort && confirm === password;

  async function submit(e: FormEvent) {
    e.preventDefault();
    if (!canSubmit) return;
    setBusy(true);
    setError(null);
    try {
      await api.create(password);
      setPassword("");
      setConfirm("");
      onCreated();
    } catch (err) {
      setError({ e: err });
      setBusy(false);
    }
  }

  return (
    <form className="card narrow" onSubmit={submit}>
      <h1>{t.createTitle}</h1>
      <p className="muted">{t.createIntro(autolockMinutes)}</p>
      <label>
        {t.password}
        <input
          type="password"
          autoFocus
          autoComplete="new-password"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          disabled={busy}
        />
      </label>
      <span className={`hint ${password && tooShort ? "bad" : ""}`}>
        {t.passwordRule(MIN_PASSWORD_CHARS)}
      </span>
      <label>
        {t.confirmPassword}
        <input
          type="password"
          autoComplete="new-password"
          value={confirm}
          onChange={(e) => setConfirm(e.target.value)}
          disabled={busy}
        />
      </label>
      {mismatch && <span className="hint bad">{t.mismatch}</span>}
      <div className="warning">
        <strong>{t.noRecoveryStrong}</strong> {t.noRecoveryCreate}
      </div>
      {error && <div className="result error">{errorText(error.e)}</div>}
      <button className="primary" type="submit" disabled={!canSubmit}>
        {busy ? t.creating : t.createTitle}
      </button>
    </form>
  );
}
