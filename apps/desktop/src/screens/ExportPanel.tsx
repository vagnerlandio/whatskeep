import { useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { chatTitle, formatCount, formatSize, Icon, IconName, useLang } from "@wk/viewer";
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
import { Badge, FlowHeader, Note, OptionCard, Panel, PanelActions, Section, Segmented } from "./ui";

type Step =
  | { kind: "form"; error?: { e: unknown } }
  | { kind: "running"; progress: ExportProgress | null }
  | { kind: "done"; summary: ExportSummary; path: string };

type Scope = "all" | "some";
type PasswordMode = "one" | "perChat";

const strong = (p: string) => [...p].length >= MIN_PASSWORD_CHARS;
const MEDIA: ExportMedia[] = ["encrypted", "plain", "none"];
const MEDIA_ICON: Record<ExportMedia, IconName> = { encrypted: "lock", plain: "image", none: "document" };

/** Generates the offline HTML history, encrypted with a password. */
export default function ExportPanel({ stores, onBack }: { stores: StoreInfo[]; onBack: () => void }) {
  const t = useAppT();
  const { lang } = useLang();
  const errorText = useErrorText();
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
    viewerApi.chats(store).then(setChats, (e) => setStep({ kind: "form", error: { e } }));
    setPicked(new Set());
    setPerChat({});
  }, [store]);

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
  const cloudBlocked = media === "plain" && !!dest?.cloud;
  const canRun = selected.length > 0 && passwordsOk && dest !== null && !cloudBlocked;

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

  function toggle(id: number, on: boolean) {
    const next = new Set(picked);
    if (on) next.add(id);
    else next.delete(id);
    setPicked(next);
  }

  if (step.kind === "running") {
    const p = step.progress;
    const pct = p && p.chatsTotal > 0 ? (p.chatsDone / p.chatsTotal) * 100 : 0;
    return (
      <div className="flow">
        <Panel>
          <Section>
            <div className="job-head">
              <Badge icon="document" />
              <div>
                <h2>{t.exporting}</h2>
                <p className="muted small">{SOURCE_LABEL[store]}</p>
              </div>
            </div>
            <div className="bar">
              <div style={{ width: `${pct}%` }} />
            </div>
            <span className="muted small">
              {p ? t.exportProgress(p.chatsDone, p.chatsTotal, p.messages, p.mediaFiles, p.mediaBytes) : t.preparing}
            </span>
          </Section>
          <PanelActions>
            <button className="ghost-danger" onClick={() => void exportApi.cancel()}>
              {t.cancel}
            </button>
          </PanelActions>
        </Panel>
      </div>
    );
  }

  if (step.kind === "done") {
    const s = step.summary;
    const n = (v: number) => formatCount(v, lang);
    return (
      <div className="flow">
        <Panel>
          <Section>
            <div className="job-head">
              <Badge icon="checkCircle" />
              <div>
                <h2>{t.exportDone}</h2>
                <p className="muted small">{SOURCE_LABEL[store]}</p>
              </div>
            </div>
            <div className="stats">
              <div className="stat">
                <strong>{n(s.chats)}</strong>
                <span>{t.statChats}</span>
              </div>
              <div className="stat">
                <strong>{n(s.messages)}</strong>
                <span>{t.statMessages}</span>
              </div>
              <div className="stat">
                <strong>{n(s.mediaFiles)}</strong>
                <span>{t.statMedia(formatSize(s.mediaBytes, lang))}</span>
              </div>
            </div>
          </Section>
          <Section>
            <div className="facts">
              <div className="fact">
                <Icon name="folder" size={20} />
                <span className="fact-title">{t.savedTo}</span>
                <span className="fact-detail mono-path">{step.path}</span>
              </div>
              <div className="fact">
                <Icon name="info" size={20} />
                <span className="fact-title">{t.howToOpen}</span>
                <span className="fact-detail">
                  {t.doneOpen} {t.doneCopy}
                </span>
              </div>
            </div>
            {s.mediaMissing > 0 && <Note tone="warn">{t.mediaMissing(s.mediaMissing)}</Note>}
            <Note tone="warn" icon="key">
              {t.doneNoRecovery}
            </Note>
          </Section>
          <PanelActions>
            <button onClick={() => setStep({ kind: "form" })}>{t.newExport}</button>
            <button className="primary" onClick={onBack}>
              {t.close}
            </button>
          </PanelActions>
        </Panel>
      </div>
    );
  }

  return (
    <div className="flow">
      <FlowHeader title={t.exportTitle} subtitle={t.exportShort} onBack={onBack} backLabel={t.back} />
      <Panel>
        {stores.length > 1 && (
          <Section label={t.history}>
            <Segmented
              value={store}
              options={stores.map((s) => ({ value: s.sourceApp, label: SOURCE_LABEL[s.sourceApp] }))}
              onChange={setStore}
            />
          </Section>
        )}

        <Section label={t.chatsLegend} aside={scope === "some" && <span className="muted small">{t.picked(picked.size)}</span>}>
          <Segmented
            value={scope}
            options={[
              { value: "all", label: t.allChats(chats.length) },
              { value: "some", label: t.pickChats },
            ]}
            onChange={setScope}
          />
          {scope === "some" && (
            <>
              <div className="search-field">
                <Icon name="search" size={16} />
                <input type="search" placeholder={t.filter} value={filter} onChange={(e) => setFilter(e.target.value)} />
              </div>
              <div className="check-list">
                {visible.map((c) => (
                  <label key={c.id} className="check-row">
                    <input type="checkbox" checked={picked.has(c.id)} onChange={(e) => toggle(c.id, e.target.checked)} />
                    <span className="pick-title">{chatTitle(c)}</span>
                    <span className="muted small">{t.messages(c.messageCount)}</span>
                  </label>
                ))}
              </div>
              <p className="muted small">{t.pickNote}</p>
            </>
          )}
        </Section>

        <Section label={t.passwordLegend}>
          <Segmented
            value={mode}
            options={[
              { value: "one", label: t.onePassword },
              { value: "perChat", label: t.perChatShort },
            ]}
            onChange={setMode}
          />
          {mode === "one" ? (
            <div className="field-pair">
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
            </div>
          ) : (
            <>
              <p className="muted small">{t.perChatHint}</p>
              <div className="check-list">
                {selected.map((c) => (
                  <label key={c.id} className="check-row">
                    <span className="pick-title">{chatTitle(c)}</span>
                    <input
                      type="password"
                      autoComplete="new-password"
                      placeholder={t.passwordPlaceholder(MIN_PASSWORD_CHARS)}
                      value={perChat[c.id] ?? ""}
                      className={perChat[c.id] && !strong(perChat[c.id]) ? "bad" : ""}
                      onChange={(e) => setPerChat({ ...perChat, [c.id]: e.target.value })}
                    />
                  </label>
                ))}
              </div>
            </>
          )}
          {mode === "one" && confirm && confirm !== password && <span className="hint bad">{t.mismatch}</span>}
          <Note tone="warn" icon="key">
            {t.noRecovery}
          </Note>
        </Section>

        <Section label={t.mediaLegend}>
          <div className="option-list">
            {MEDIA.map((m) => (
              <OptionCard
                key={m}
                name="export-media"
                selected={media === m}
                onSelect={() => setMedia(m)}
                icon={MEDIA_ICON[m]}
                title={t.mediaOptTitle[m]}
                hint={t.mediaOptHint[m]}
              />
            ))}
          </div>
        </Section>

        <Section label={t.destination}>
          <div className="file-pick">
            <Icon name="folder" size={20} />
            <span className="file-pick-name">
              {dest ? <span className="mono-path">{dest.path}</span> : <span className="muted">{t.noFolder}</span>}
            </span>
            <button className="small-btn" onClick={chooseDest}>
              {t.chooseFolder}
            </button>
          </div>
          {cloudBlocked && dest?.cloud && <Note tone="warn">{t.cloudWarning(dest.cloud)}</Note>}
          {step.error && <Note tone="danger">{errorText(step.error.e)}</Note>}
        </Section>

        <PanelActions>
          <button className="primary" disabled={!canRun} onClick={run}>
            {t.exportButton(selected.length)}
          </button>
        </PanelActions>
      </Panel>
    </div>
  );
}
