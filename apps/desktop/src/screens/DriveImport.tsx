import { formatDateTime, Icon, useLang } from "@wa/viewer";
import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  driveApi,
  DriveBackup,
  DriveLogin,
  errorOf,
  formatCount,
  formatSize,
  ImportPhase,
  ImportSummary,
  mediaApi,
  MediaProgress,
  MediaSummary,
  SOURCE_LABEL,
  SourceApp,
  StoreInfo,
} from "../api";
import { formatPhone } from "@wa/viewer";
import { useErrorText } from "../i18n/errors";
import { useImportT } from "../i18n/import";
import { ReplaceWarning } from "./ImportPanel";
import {
  initialKey,
  KeyField,
  KeyState,
  keyReady,
  MediaProgressView,
  SourcePicker,
  SummaryView,
  toKeyInput,
  useFriendlyError,
  usePhaseLabel,
} from "./shared";

type Step =
  | { kind: "intro"; error?: string }
  | { kind: "waiting" }
  | { kind: "manual"; error?: string }
  | { kind: "list"; login: DriveLogin; error?: string }
  | { kind: "running"; phase: ImportPhase; downloaded: number; total: number; media: MediaProgress | null }
  | { kind: "done"; source: SourceApp; summary: ImportSummary; media: MediaSummary | null };

type Props = { stores: StoreInfo[]; onImported: () => void };

/** Google sign-in and backup download straight from Google Drive. */
export default function DriveImport({ stores, onImported }: Props) {
  const [step, setStep] = useState<Step>({ kind: "intro" });
  const [email, setEmail] = useState("");
  const [manualToken, setManualToken] = useState("");
  const [selected, setSelected] = useState<DriveBackup | null>(null);
  const [source, setSource] = useState<SourceApp>("whatsapp");
  const [key, setKey] = useState<KeyState>(initialKey("crypt15"));
  const [includeMedia, setIncludeMedia] = useState(false);
  const sessionOpen = useRef(false);
  const t = useImportT();
  const { lang } = useLang();
  const phaseLabel = usePhaseLabel();
  const errorText = useErrorText();
  const friendlyError = useFriendlyError();

  // When leaving the screen, the Google tokens are discarded in the backend.
  useEffect(
    () => () => {
      if (sessionOpen.current) void driveApi.cancel();
    },
    [],
  );

  function loggedIn(login: DriveLogin) {
    sessionOpen.current = true;
    setManualToken("");
    const first = login.backups.find((b) => !b.uploading) ?? null;
    pick(first);
    setStep({ kind: "list", login });
  }

  function pick(b: DriveBackup | null) {
    setSelected(b);
    if (!b) return;
    setSource(b.apps.length === 1 ? b.apps[0] : "whatsapp");
    setKey(initialKey(b.encrypted === false ? "crypt14" : "crypt15"));
  }

  async function login() {
    setStep({ kind: "waiting" });
    try {
      loggedIn(await driveApi.login(email.trim()));
    } catch (e) {
      const { code } = errorOf(e);
      setStep({ kind: "intro", error: code === "cancelled" ? undefined : errorText(e) });
    }
  }

  async function loginManual() {
    try {
      loggedIn(await driveApi.loginManual(email.trim(), manualToken));
    } catch (e) {
      setManualToken("");
      setStep({ kind: "manual", error: errorText(e) });
    }
  }

  async function cancel() {
    sessionOpen.current = false;
    await driveApi.cancel();
    setStep({ kind: "intro" });
  }

  async function download() {
    if (step.kind !== "list" || !selected) return;
    const login = step.login;
    const input = toKeyInput(key);
    setKey((k) => ({ ...k, hex: "" }));
    setStep({ kind: "running", phase: "downloading", downloaded: 0, total: selected.chatDbSize ?? 0, media: null });
    const unlisteners = await Promise.all([
      listen<{ phase: ImportPhase }>("import-progress", (e) =>
        setStep((s) => (s.kind === "running" ? { ...s, phase: e.payload.phase } : s)),
      ),
      listen<{ downloaded: number; total: number }>("gdrive-progress", (e) =>
        setStep((s) => (s.kind === "running" ? { ...s, ...e.payload } : s)),
      ),
      listen<MediaProgress>("media-progress", (e) =>
        setStep((s) => (s.kind === "running" ? { ...s, media: e.payload } : s)),
      ),
    ]);
    try {
      const { summary, media } = await driveApi.download(selected.id, input, source, includeMedia);
      sessionOpen.current = false;
      setStep({ kind: "done", source, summary, media });
      onImported();
    } catch (e) {
      const { code } = errorOf(e);
      // Network: the session stays alive in the backend and can be retried. Other errors end the session.
      if (code === "network" || code === "download_corrupt") {
        setStep({ kind: "list", login, error: errorText(e) });
      } else {
        sessionOpen.current = false;
        setStep({ kind: "intro", error: code === "cancelled" ? undefined : friendlyError(e) });
      }
    } finally {
      unlisteners.forEach((u) => u());
    }
  }

  if (step.kind === "done") {
    return (
      <SummaryView
        source={step.source}
        summary={step.summary}
        media={step.media}
        footer={t.driveFooter}
        onAgain={() => setStep({ kind: "intro" })}
      />
    );
  }

  if (step.kind === "running") {
    const pct = step.total > 0 ? Math.min(100, (step.downloaded / step.total) * 100) : 0;
    const mediaPhase = step.phase === "done" && includeMedia;
    return (
      <section className="card">
        <h2>{t.drive}</h2>
        <div className="progress">
          <div className="spinner" />
          <span>{mediaPhase ? t.downloadingMedia : phaseLabel[step.phase]}</span>
        </div>
        {step.phase === "downloading" && (
          <>
            <div className="bar">
              <div style={{ width: `${pct}%` }} />
            </div>
            <span className="muted small">
              {formatSize(step.downloaded, lang)}
              {step.total > 0 && t.ofTotal(formatSize(step.total, lang))}
            </span>
            <button onClick={cancel}>{t.cancel}</button>
          </>
        )}
        {mediaPhase && (
          <MediaProgressView
            p={step.media}
            onCancel={() => {
              void mediaApi.cancel();
              void driveApi.cancel();
            }}
          />
        )}
        <p className="muted small">{t.driveInMemory}</p>
      </section>
    );
  }

  if (step.kind === "waiting") {
    return (
      <section className="card">
        <h2>{t.drive}</h2>
        <div className="progress">
          <div className="spinner" />
          <span>{t.waitingLogin}</span>
        </div>
        <p className="muted small">{t.waitingHint}</p>
        <button onClick={cancel}>{t.cancel}</button>
      </section>
    );
  }

  if (step.kind === "manual") {
    return (
      <section className="card">
        <h2>{t.manualTitle}</h2>
        <ol className="steps">
          <li>
            {t.manualStep1Before}
            <code>https://accounts.google.com/EmbeddedSetup</code>
            {t.manualStep1After(email || t.manualAccountFallback)}
          </li>
          <li>
            {t.manualStep2Before}
            <code>accounts.google.com</code>
            {t.manualStep2After}
          </li>
          <li>
            {t.manualStep3Before}
            <code>oauth_token</code>
            {t.manualStep3Middle}
            <code>oauth2_4/</code>
            {t.manualStep3After}
          </li>
        </ol>
        <textarea
          className="key-input"
          rows={3}
          spellCheck={false}
          autoComplete="off"
          placeholder="oauth2_4/…"
          value={manualToken}
          onChange={(e) => setManualToken(e.target.value)}
        />
        {step.error && <div className="result error">{step.error}</div>}
        <div className="row-buttons">
          <button onClick={() => setStep({ kind: "intro" })}>{t.back}</button>
          <button className="primary" disabled={!manualToken.trim() || !email.includes("@")} onClick={loginManual}>
            {t.continue}
          </button>
        </div>
      </section>
    );
  }

  if (step.kind === "list") {
    const { backups, refused } = step.login;
    const existing = stores.find((s) => s.sourceApp === source);
    return (
      <section className="card">
        <div className="file-row">
          <h2>{t.driveBackups}</h2>
          <button onClick={cancel}>{t.signOut}</button>
        </div>
        {backups.length === 0 && <p className="muted">{t.noBackups}</p>}
        <div className="drive-list">
          {backups.map((b) => (
            <label key={b.id} className={`drive-item ${b.uploading ? "disabled" : ""}`}>
              <input
                type="radio"
                name="drive-backup"
                disabled={b.uploading}
                checked={selected?.id === b.id}
                onChange={() => pick(b)}
              />
              <span className="drive-item-body">
                <strong>
                  {b.phone ? formatPhone(b.phone) : t.backup}{" "}
                  {b.apps.map((a) => (
                    <span key={a} className="badge">
                      {SOURCE_LABEL[a]}
                    </span>
                  ))}
                  {b.encrypted && (
                    <span className="badge">
                      <Icon name="lock" size={12} /> {t.endToEnd}
                    </span>
                  )}
                </strong>
                <span className="muted small">
                  {b.uploading
                    ? t.stillUploading
                    : [
                        b.updateTime && formatDateTime(new Date(b.updateTime).getTime(), lang),
                        b.chatDbSize != null && t.chatDbSize(formatSize(b.chatDbSize, lang)),
                        b.messages != null && t.messagesCount(formatCount(b.messages, lang)),
                        b.sizeBytes != null && t.totalSize(formatSize(b.sizeBytes, lang)),
                      ]
                        .filter(Boolean)
                        .join(" · ")}
                </span>
              </span>
            </label>
          ))}
        </div>
        {refused.length > 0 && backups.length > 0 && (
          <p className="muted small">{t.refused(refused.map((a) => SOURCE_LABEL[a]).join(t.and))}</p>
        )}
        {selected && (
          <>
            <SourcePicker value={source} onChange={setSource} />
            {selected.encrypted === false && (
              <div className="warning">
                <strong>{t.noE2eTitle}</strong>
                {t.noE2eBefore}
                <code>key</code>
                {t.noE2eAfter}
                <ol>
                  <li>
                    {t.noE2eStep1Before}
                    <em>{t.noE2eStep1Path}</em>
                    {t.noE2eStep1After}
                  </li>
                  <li>
                    {t.noE2eStep2Before}
                    <em>{t.noE2eStep2Option}</em>
                    {t.noE2eStep2After}
                  </li>
                  <li>{t.noE2eStep3}</li>
                  <li>{t.noE2eStep4}</li>
                </ol>
                {t.noE2eBusiness}
              </div>
            )}
            <KeyField version={selected.encrypted === false ? "crypt14" : "crypt15"} value={key} onChange={setKey} />
            {existing && <ReplaceWarning source={source} at={existing.importedAtMs} />}
            <label className="radio">
              <input type="checkbox" checked={includeMedia} onChange={(e) => setIncludeMedia(e.target.checked)} />
              {t.includeMedia}
              {selected.mediaSize != null && t.upTo(formatSize(selected.mediaSize, lang))}
            </label>
            {includeMedia && (
              <p className="muted small">{t.includeMediaHint}</p>
            )}
          </>
        )}
        {step.error && <div className="result error">{step.error}</div>}
        <button className="primary" disabled={!selected || !keyReady(key)} onClick={download}>
          {t.downloadAndImport}
        </button>
      </section>
    );
  }

  return (
    <section className="card">
      <h2>{t.driveDownload}</h2>
      <ul className="muted small bullets">
        <li>
          {t.introUnofficialBefore}
          <strong>{t.introUnofficial}</strong>
          {t.introUnofficialAfter}
        </li>
        <li>{t.introMemory}</li>
        <li>{t.introPassword}</li>
      </ul>
      <label>
        {t.emailLabel}
        <input
          type="email"
          autoComplete="email"
          spellCheck={false}
          value={email}
          onChange={(e) => setEmail(e.target.value)}
        />
      </label>
      <span className="muted small">{t.emailHint}</span>
      {step.error && <div className="result error">{step.error}</div>}
      <button className="primary" disabled={!email.includes("@")} onClick={login}>
        {t.signIn}
      </button>
      <p className="muted small">
        {t.refusedWindow}{" "}
        <button className="link" disabled={!email.includes("@")} onClick={() => setStep({ kind: "manual" })}>
          {t.manualLogin}
        </button>
      </p>
    </section>
  );
}
