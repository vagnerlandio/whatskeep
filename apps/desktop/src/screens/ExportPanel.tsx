import { useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { chatTitle } from "@wa/viewer";
import type { Chat } from "@wa/viewer";
import {
  errorOf,
  exportApi,
  ExportMedia,
  ExportProgress,
  ExportSummary,
  formatCount,
  formatSize,
  MIN_PASSWORD_CHARS,
  SOURCE_LABEL,
  SourceApp,
  StoreInfo,
  viewerApi,
} from "../api";

type Step =
  | { kind: "form"; error?: string }
  | { kind: "running"; progress: ExportProgress | null }
  | { kind: "done"; summary: ExportSummary; path: string };

type Scope = "all" | "some";
type PasswordMode = "one" | "perChat";

const strong = (p: string) => [...p].length >= MIN_PASSWORD_CHARS;

/** Gera o histórico em HTML offline, cifrado por senha. */
export default function ExportPanel({ stores }: { stores: StoreInfo[] }) {
  const [open, setOpen] = useState(false);
  const [store, setStore] = useState<SourceApp>(stores[0]?.sourceApp ?? "whatsapp");
  const [chats, setChats] = useState<Chat[]>([]);
  const [scope, setScope] = useState<Scope>("all");
  const [picked, setPicked] = useState<Set<number>>(new Set());
  const [filter, setFilter] = useState("");
  const [mode, setMode] = useState<PasswordMode>("one");
  const [password, setPassword] = useState("");
  const [confirm, setConfirm] = useState("");
  const [perChat, setPerChat] = useState<Record<number, string>>({});
  const [media, setMedia] = useState<ExportMedia>("encrypted");
  const [dest, setDest] = useState<{ path: string; cloud: string | null } | null>(null);
  const [step, setStep] = useState<Step>({ kind: "form" });

  useEffect(() => {
    if (!open) return;
    viewerApi.chats(store).then(setChats, (e) => setStep({ kind: "form", error: errorOf(e).message }));
    setPicked(new Set());
    setPerChat({});
  }, [open, store]);

  const selected = useMemo(
    () => (scope === "all" ? chats : chats.filter((c) => picked.has(c.id))),
    [scope, chats, picked],
  );
  const visible = useMemo(() => {
    const f = filter.trim().toLowerCase();
    return f ? chats.filter((c) => chatTitle(c).toLowerCase().includes(f)) : chats;
  }, [chats, filter]);

  const passwordsOk =
    mode === "one"
      ? strong(password) && password === confirm
      : selected.length > 0 && selected.every((c) => strong(perChat[c.id] ?? ""));
  const canRun = selected.length > 0 && passwordsOk && dest !== null;

  async function chooseDest() {
    try {
      const d = await exportApi.pickDestination();
      if (d) setDest(d);
    } catch (e) {
      setStep({ kind: "form", error: errorOf(e).message });
    }
  }

  async function run() {
    const groups =
      mode === "one"
        ? [{ password, chats: selected.map((c) => c.id) }]
        : selected.map((c) => ({ password: perChat[c.id], chats: [c.id] }));
    // Senhas não ficam no estado do React depois de enviadas.
    setPassword("");
    setConfirm("");
    setPerChat({});
    setStep({ kind: "running", progress: null });
    const unlisten = await listen<ExportProgress>("export-progress", (e) =>
      setStep((s) => (s.kind === "running" ? { kind: "running", progress: e.payload } : s)),
    );
    try {
      const r = await exportApi.run(store, groups, media);
      setDest(null);
      setStep({ kind: "done", summary: r.summary, path: r.path });
    } catch (e) {
      const { code, message } = errorOf(e);
      setDest(null); // a pasta é recriada/escolhida de novo
      setStep({ kind: "form", error: code === "cancelled" ? undefined : message });
    } finally {
      unlisten();
    }
  }

  if (!open) {
    return (
      <section className="card">
        <h2>Exportar HTML</h2>
        <p className="muted small">
          Gera uma pasta com o histórico que abre em qualquer navegador, sem este app e sem internet. As mensagens
          ficam cifradas com a senha que você definir.
        </p>
        <button onClick={() => setOpen(true)} disabled={stores.length === 0}>
          Configurar exportação…
        </button>
      </section>
    );
  }

  if (step.kind === "running") {
    const p = step.progress;
    const pct = p && p.chatsTotal > 0 ? (p.chatsDone / p.chatsTotal) * 100 : 0;
    return (
      <section className="card">
        <h2>Exportando</h2>
        <div className="bar">
          <div style={{ width: `${pct}%` }} />
        </div>
        <span className="muted small">
          {p
            ? `${formatCount(p.chatsDone)} de ${formatCount(p.chatsTotal)} conversas · ${formatCount(p.messages)} mensagens · ${formatCount(p.mediaFiles)} mídias (${formatSize(p.mediaBytes)})`
            : "Preparando…"}
        </span>
        <button onClick={() => void exportApi.cancel()}>Cancelar</button>
      </section>
    );
  }

  if (step.kind === "done") {
    const s = step.summary;
    return (
      <section className="card">
        <h2>Exportação concluída</h2>
        <div className="result">
          <strong>{step.path}</strong>
          <span>
            {formatCount(s.chats)} conversas · {formatCount(s.messages)} mensagens · {formatCount(s.mediaFiles)} mídias (
            {formatSize(s.mediaBytes)})
          </span>
          {s.mediaMissing > 0 && (
            <span className="muted small">
              {formatCount(s.mediaMissing)} mídias citadas nas mensagens não estão no cofre e entraram só como miniatura.
            </span>
          )}
        </div>
        <ul className="muted small bullets">
          <li>Abra o arquivo index.html dessa pasta no navegador e digite a senha.</li>
          <li>Para levar a outro computador, copie a pasta inteira.</li>
          <li>Não existe recuperação: sem a senha, ninguém abre o histórico (nem você).</li>
        </ul>
        <button onClick={() => setStep({ kind: "form" })}>Nova exportação</button>
      </section>
    );
  }

  return (
    <section className="card">
      <div className="file-row">
        <h2>Exportar HTML</h2>
        <button onClick={() => setOpen(false)}>Fechar</button>
      </div>

      {stores.length > 1 && (
        <fieldset>
          <legend>Histórico</legend>
          {stores.map((s) => (
            <label key={s.sourceApp} className="radio">
              <input type="radio" checked={store === s.sourceApp} onChange={() => setStore(s.sourceApp)} />
              {SOURCE_LABEL[s.sourceApp]}
            </label>
          ))}
        </fieldset>
      )}

      <fieldset>
        <legend>Conversas</legend>
        <label className="radio">
          <input type="radio" checked={scope === "all"} onChange={() => setScope("all")} />
          Todas ({formatCount(chats.length)})
        </label>
        <label className="radio">
          <input type="radio" checked={scope === "some"} onChange={() => setScope("some")} />
          Escolher conversas {scope === "some" && `(${formatCount(picked.size)} escolhidas)`}
        </label>
        {scope === "some" && (
          <div className="pick-list">
            <input
              type="search"
              className="pick-filter"
              placeholder="Filtrar"
              value={filter}
              onChange={(e) => setFilter(e.target.value)}
            />
            <div className="pick-items">
              {visible.map((c) => (
                <label key={c.id} className="radio">
                  <input
                    type="checkbox"
                    checked={picked.has(c.id)}
                    onChange={(e) => {
                      const next = new Set(picked);
                      if (e.target.checked) next.add(c.id);
                      else next.delete(c.id);
                      setPicked(next);
                    }}
                  />
                  <span className="pick-title">{chatTitle(c)}</span>
                  <span className="muted small">{formatCount(c.messageCount)}</span>
                </label>
              ))}
            </div>
            <p className="muted small">As conversas que não forem escolhidas não entram em nada no pacote.</p>
          </div>
        )}
      </fieldset>

      <fieldset>
        <legend>Senha</legend>
        <label className="radio">
          <input type="radio" checked={mode === "one"} onChange={() => setMode("one")} />
          Uma senha para todas
        </label>
        <label className="radio">
          <input type="radio" checked={mode === "perChat"} onChange={() => setMode("perChat")} />
          Uma senha por conversa (para entregar uma conversa a alguém)
        </label>
        {mode === "one" ? (
          <>
            <input
              type="password"
              autoComplete="new-password"
              placeholder={`Senha (mínimo ${MIN_PASSWORD_CHARS} caracteres)`}
              value={password}
              onChange={(e) => setPassword(e.target.value)}
            />
            <input
              type="password"
              autoComplete="new-password"
              placeholder="Confirmar senha"
              value={confirm}
              onChange={(e) => setConfirm(e.target.value)}
            />
            {confirm && confirm !== password && <span className="hint bad">As senhas não conferem.</span>}
          </>
        ) : (
          <div className="pick-items">
            {selected.map((c) => (
              <label key={c.id} className="per-chat">
                <span className="pick-title">{chatTitle(c)}</span>
                <input
                  type="password"
                  autoComplete="new-password"
                  value={perChat[c.id] ?? ""}
                  className={perChat[c.id] && !strong(perChat[c.id]) ? "bad" : ""}
                  onChange={(e) => setPerChat({ ...perChat, [c.id]: e.target.value })}
                />
              </label>
            ))}
          </div>
        )}
        <span className="hint">Não existe recuperação de senha.</span>
      </fieldset>

      <fieldset>
        <legend>Fotos, vídeos e áudios</legend>
        <label className="radio">
          <input type="radio" checked={media === "encrypted"} onChange={() => setMedia("encrypted")} />
          Cifrados (recomendado). No navegador, você seleciona a pasta uma vez para vê-los.
        </label>
        <label className="radio">
          <input type="radio" checked={media === "plain"} onChange={() => setMedia("plain")} />
          Sem cifra: abrem direto, mas qualquer pessoa com acesso à pasta vê os arquivos.
        </label>
        <label className="radio">
          <input type="radio" checked={media === "none"} onChange={() => setMedia("none")} />
          Só miniaturas (pacote bem menor)
        </label>
      </fieldset>

      <fieldset>
        <legend>Destino</legend>
        <div className="file-row">
          <span className="muted small dest-path">{dest ? dest.path : "Nenhuma pasta escolhida"}</span>
          <button onClick={chooseDest}>Escolher pasta…</button>
        </div>
        {dest?.cloud && media === "plain" && (
          <div className="warning">
            Esta pasta está no {dest.cloud}: as mídias sem cifra seriam enviadas para a nuvem. Escolha outra pasta ou
            use mídias cifradas.
          </div>
        )}
      </fieldset>

      {step.error && <div className="result error">{step.error}</div>}
      <button className="primary" disabled={!canRun || (media === "plain" && !!dest?.cloud)} onClick={run}>
        Exportar {formatCount(selected.length)} {selected.length === 1 ? "conversa" : "conversas"}
      </button>
    </section>
  );
}
