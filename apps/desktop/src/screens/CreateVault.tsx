import { FormEvent, useState } from "react";
import { api, errorOf, MIN_PASSWORD_CHARS } from "../api";

type Props = { autolockMinutes: number; onCreated: () => void };

export default function CreateVault({ autolockMinutes, onCreated }: Props) {
  const [password, setPassword] = useState("");
  const [confirm, setConfirm] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

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
      setError(errorOf(err).message);
      setBusy(false);
    }
  }

  return (
    <form className="card narrow" onSubmit={submit}>
      <h1>Criar cofre</h1>
      <p className="muted">
        O histórico importado fica guardado neste computador, cifrado com esta senha. Ela é pedida ao abrir o app e
        depois de {autolockMinutes} minutos sem uso.
      </p>
      <label>
        Senha
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
        Mínimo de {MIN_PASSWORD_CHARS} caracteres. Uma frase longa é mais forte que uma senha curta complicada.
      </span>
      <label>
        Confirmar senha
        <input
          type="password"
          autoComplete="new-password"
          value={confirm}
          onChange={(e) => setConfirm(e.target.value)}
          disabled={busy}
        />
      </label>
      {mismatch && <span className="hint bad">As senhas não conferem.</span>}
      <div className="warning">
        <strong>Não existe recuperação de senha.</strong> Se você esquecer, o cofre não pode ser aberto e será preciso
        importar os backups de novo.
      </div>
      {error && <div className="result error">{error}</div>}
      <button className="primary" type="submit" disabled={!canSubmit}>
        {busy ? "Criando cofre…" : "Criar cofre"}
      </button>
    </form>
  );
}
