import { useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { chatTitle, formatCount, useLang } from "@wk/viewer";
import type { Chat } from "@wk/viewer";
import {
  errorOf,
  exportApi,
  ExportMedia,
  ExportProgress,
  ExportSummary,
  MIN_PASSWORD_CHARS,
  SOURCE_LABEL,
  SourceApp,
  StoreInfo,
  viewerApi,
} from "../api";
import { useAppT } from "../i18n/app";
import { useErrorText } from "../i18n/errors";

type Step =
  | { kind: "form"; error?: { e: unknown } }
  | { kind: "running"; progress: ExportProgress | null }
  | { kind: "done"; summary: ExportSummary; path: string };

type Scope = "all" | "some";
type PasswordMode = "one" | "perChat";

const strong = (p: string) => [...p].length >= MIN_PASSWORD_CHARS;

/** Generates the offline HTML history, encrypted with a password. */
export default function ExportPanel({ stores }: { stores: StoreInfo[] }) {
  const t = useAppT();
  const { lang } = useLang();
  const errorText = useErrorText();
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
    viewerApi.chats(store).then(setChats, (e) => setStep({ kind: "form", error: { e } }));
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
      setStep({ kind: "form", error: { e } });
    }
  }

  async function run() {
    const groups =
      mode === "one"
        ? [{ password, chats: selected.map((c) => c.id) }]
        : selected.map((c) => ({ password: perChat[c.id], chats: [c.id] }));
    // Passwords don't stay in React state after being sent.
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
      const { code } = errorOf(e);
      setDest(null); // the folder is recreated/picked again
      setStep({ kind: "form", error: code === "cancelled" ? undefined : { e } });
    } finally {
      unlisten();
    }
  }

  if (!open) {
    return (
      <section className="card">
        <h2>{t.exportTitle}</h2>
        <p className="muted small">{t.exportIntro}</p>
        <button onClick={() => setOpen(true)} disabled={stores.length === 0}>
          {t.configureExport}
        </button>
      </section>
    );
  }

  if (step.kind === "running") {
    const p = step.progress;
    const pct = p && p.chatsTotal > 0 ? (p.chatsDone / p.chatsTotal) * 100 : 0;
    return (
      <section className="card">
        <h2>{t.exporting}</h2>
        <div className="bar">
          <div style={{ width: `${pct}%` }} />
        </div>
        <span className="muted small">
          {p
            ? t.exportProgress(p.chatsDone, p.chatsTotal, p.messages, p.mediaFiles, p.mediaBytes)
            : t.preparing}
        </span>
        <button onClick={() => void exportApi.cancel()}>{t.cancel}</button>
      </section>
    );
  }

  if (step.kind === "done") {
    const s = step.summary;
    return (
      <section className="card">
        <h2>{t.exportDone}</h2>
        <div className="result">
          <strong>{step.path}</strong>
          <span>{t.exportSummary(s.chats, s.messages, s.mediaFiles, s.mediaBytes)}</span>
          {s.mediaMissing > 0 && (
            <span className="muted small">{t.mediaMissing(s.mediaMissing)}</span>
          )}
        </div>
        <ul className="muted small bullets">
          <li>{t.doneOpen}</li>
          <li>{t.doneCopy}</li>
          <li>{t.doneNoRecovery}</li>
        </ul>
        <button onClick={() => setStep({ kind: "form" })}>{t.newExport}</button>
      </section>
    );
  }

  return (
    <section className="card">
      <div className="file-row">
        <h2>{t.exportTitle}</h2>
        <button onClick={() => setOpen(false)}>{t.close}</button>
      </div>

      {stores.length > 1 && (
        <fieldset>
          <legend>{t.history}</legend>
          {stores.map((s) => (
            <label key={s.sourceApp} className="radio">
              <input type="radio" checked={store === s.sourceApp} onChange={() => setStore(s.sourceApp)} />
              {SOURCE_LABEL[s.sourceApp]}
            </label>
          ))}
        </fieldset>
      )}

      <fieldset>
        <legend>{t.chatsLegend}</legend>
        <label className="radio">
          <input type="radio" checked={scope === "all"} onChange={() => setScope("all")} />
          {t.allChats(chats.length)}
        </label>
        <label className="radio">
          <input type="radio" checked={scope === "some"} onChange={() => setScope("some")} />
          {t.pickChats} {scope === "some" && t.picked(picked.size)}
        </label>
        {scope === "some" && (
          <div className="pick-list">
            <input
              type="search"
              className="pick-filter"
              placeholder={t.filter}
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
                  <span className="muted small">{formatCount(c.messageCount, lang)}</span>
                </label>
              ))}
            </div>
            <p className="muted small">{t.pickNote}</p>
          </div>
        )}
      </fieldset>

      <fieldset>
        <legend>{t.passwordLegend}</legend>
        <label className="radio">
          <input type="radio" checked={mode === "one"} onChange={() => setMode("one")} />
          {t.onePassword}
        </label>
        <label className="radio">
          <input type="radio" checked={mode === "perChat"} onChange={() => setMode("perChat")} />
          {t.perChatPassword}
        </label>
        {mode === "one" ? (
          <>
            <input
              type="password"
              autoComplete="new-password"
              placeholder={t.passwordPlaceholder(MIN_PASSWORD_CHARS)}
              value={password}
              onChange={(e) => setPassword(e.target.value)}
            />
            <input
              type="password"
              autoComplete="new-password"
              placeholder={t.confirmPassword}
              value={confirm}
              onChange={(e) => setConfirm(e.target.value)}
            />
            {confirm && confirm !== password && <span className="hint bad">{t.mismatch}</span>}
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
        <span className="hint">{t.noRecovery}</span>
      </fieldset>

      <fieldset>
        <legend>{t.mediaLegend}</legend>
        <label className="radio">
          <input type="radio" checked={media === "encrypted"} onChange={() => setMedia("encrypted")} />
          {t.mediaEncrypted}
        </label>
        <label className="radio">
          <input type="radio" checked={media === "plain"} onChange={() => setMedia("plain")} />
          {t.mediaPlain}
        </label>
        <label className="radio">
          <input type="radio" checked={media === "none"} onChange={() => setMedia("none")} />
          {t.mediaNone}
        </label>
      </fieldset>

      <fieldset>
        <legend>{t.destination}</legend>
        <div className="file-row">
          <span className="muted small dest-path">{dest ? dest.path : t.noFolder}</span>
          <button onClick={chooseDest}>{t.chooseFolder}</button>
        </div>
        {dest?.cloud && media === "plain" && (
          <div className="warning">{t.cloudWarning(dest.cloud)}</div>
        )}
      </fieldset>

      {step.error && <div className="result error">{errorText(step.error.e)}</div>}
      <button className="primary" disabled={!canRun || (media === "plain" && !!dest?.cloud)} onClick={run}>
        {t.exportButton(selected.length)}
      </button>
    </section>
  );
}
