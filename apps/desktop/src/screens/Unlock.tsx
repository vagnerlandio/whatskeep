import { FormEvent, useState } from "react";
import { api, errorOf } from "../api";

export default function Unlock({ reason, onUnlocked }: { reason: string | null; onUnlocked: () => void }) {
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

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
      const { code, message } = errorOf(err);
      setError(code === "wrong_password" ? "Senha incorreta." : message);
      setPassword("");
      setBusy(false);
    }
  }

  return (
    <form className="card narrow" onSubmit={submit}>
      <div className="lock-icon" aria-hidden>
        🔒
      </div>
      <h1>Cofre travado</h1>
      {reason && <p className="muted">{reason}</p>}
      <label>
        Senha do cofre
        <input
          type="password"
          autoFocus
          autoComplete="current-password"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          disabled={busy}
        />
      </label>
      {error && <div className="result error">{error}</div>}
      <button className="primary" type="submit" disabled={!password || busy}>
        {busy ? "Abrindo…" : "Abrir"}
      </button>
    </form>
  );
}
