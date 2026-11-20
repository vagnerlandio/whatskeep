import { formatDateTime, formatPhone, Icon, IconName, useLang } from "@wk/viewer";
import { useEffect, useRef, useState } from "react";
import {
  driveApi,
  DriveBackup,
  DriveLogin,
  DriveMode,
  errorOf,
  formatCount,
  formatSize,
  SOURCE_LABEL,
  SourceApp,
  StoreInfo,
} from "../api";
import { useErrorText } from "../i18n/errors";
import { useImportT } from "../i18n/import";
import { JobStep, useJob } from "../jobs";
import { useOptionalVaultGate } from "../vault";
import { ReplaceWarning } from "./ImportPanel";
import { initialKey, KeyField, KeyState, keyReady, SourcePicker, toKeyInput } from "./shared";
import { FlowHeader, More, Note, OptionCard, Panel, PanelActions, Section, SwitchRow } from "./ui";

type Step =
  | { kind: "intro"; error?: string }
  | { kind: "waiting" }
  | { kind: "manual"; error?: string }
  | { kind: "list"; login: DriveLogin };

/** Errors after which the backend ended the Google session (the others keep it for a retry). */
const SESSION_ENDED = ["cancelled", "locked", "google_expired"];

const MODES: DriveMode[] = ["vault", "folderAndVault", "folder"];
const MODE_ICON: Record<DriveMode, IconName> = { vault: "lock", folderAndVault: "import", folder: "folder" };

/** The steps the progress screen shows for each mode. */
function stepsFor(mode: DriveMode, includeMedia: boolean): JobStep[] {
  if (mode === "vault") return includeMedia ? ["download", "import", "mediaDownload"] : ["download", "import"];
  if (mode === "folder") return ["download", "files"];
  return includeMedia ? ["download", "import", "files", "media"] : ["download", "import", "files"];
}

/** `folderOnly`: the vault is locked, only a copy to a folder is possible. */
type Props = { stores: StoreInfo[]; folderOnly?: boolean; onBack: () => void; backLabel?: string };

/**
 * Google sign-in and download of a backup: straight into the vault, to a
 * folder and then the vault, or only to a folder. The download itself runs as
 * an app-wide job.
 */
export default function DriveImport({ stores, folderOnly = false, onBack, backLabel }: Props) {
  const [step, setStep] = useState<Step>({ kind: "intro" });
  const [email, setEmail] = useState("");
  const [manualToken, setManualToken] = useState("");
  const [selected, setSelected] = useState<DriveBackup | null>(null);
  const [source, setSource] = useState<SourceApp>("whatsapp");
  const [key, setKey] = useState<KeyState>(initialKey("crypt15"));
  const [includeMedia, setIncludeMedia] = useState(true);
  const [mode, setMode] = useState<DriveMode>(folderOnly ? "folder" : "vault");
  const sessionOpen = useRef(false);
  const t = useImportT();
  const { lang } = useLang();
  const errorText = useErrorText();
  const { job, start } = useJob();
  const gate = useOptionalVaultGate();
  const back = backLabel ?? t.back;

  // A Drive job that ended also ended the session (or kept it, for a retry).
  const driveJob = job?.kind === "drive" ? job : null;
  useEffect(() => {
    if (!driveJob || driveJob.status === "running") return;
    const ended = driveJob.status === "done" || SESSION_ENDED.includes(errorOf(driveJob.error).code);
    if (ended) {
      sessionOpen.current = false;
      setStep({ kind: "intro" });
    } else {
      sessionOpen.current = true;
    }
  }, [driveJob?.status]);

  // When leaving the screen, the Google tokens are discarded in the backend
  // (unless a download is using them: it ends the session itself).
  const downloading = useRef(false);
  downloading.current = job?.kind === "drive" && job.status === "running";
  useEffect(
    () => () => {
      if (sessionOpen.current && !downloading.current) void driveApi.cancel();
    },
    [],
  );

  function loggedIn(login: DriveLogin) {
    sessionOpen.current = true;
    setManualToken("");
    // A backup Google still marks as uploading can be picked too (see `uploadingNote`).
    const first = login.backups.find((b) => !b.uploading) ?? login.backups[0] ?? null;
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

  async function signOut() {
    sessionOpen.current = false;
    await driveApi.cancel();
    setStep({ kind: "intro" });
  }

  function download() {
    if (step.kind !== "list" || !selected) return;
    const backupId = selected.id;
    const chosen = { mode, source, includeMedia };
    const steps = stepsFor(chosen.mode, chosen.includeMedia);
    if (chosen.mode === "folder") {
      start("drive", chosen.source, () => driveApi.download(backupId, chosen.source, "folder", null, chosen.includeMedia), {
        steps,
        picksFolder: true,
      });
      return;
    }
    const input = toKeyInput(key);
    setKey((k) => ({ ...k, hex: "" })); // the key doesn't stay in React state after being sent
    const requireVault = gate?.requireVault ?? ((action: () => void) => action());
    requireVault(() =>
      start("drive", chosen.source, () => driveApi.download(backupId, chosen.source, chosen.mode, input, chosen.includeMedia), {
        steps,
        picksFolder: chosen.mode !== "vault",
      }),
    );
  }

  if (step.kind === "waiting") {
    return (
      <div className="flow">
        <FlowHeader title={t.driveDownload} />
        <Panel>
          <Section>
            <div className="progress">
              <div className="spinner" />
              <strong>{t.waitingLogin}</strong>
            </div>
            <p className="muted small">{t.waitingHint}</p>
          </Section>
          <PanelActions>
            <button onClick={signOut}>{t.cancel}</button>
          </PanelActions>
        </Panel>
      </div>
    );
  }

  if (step.kind === "manual") {
    return (
      <div className="flow">
        <FlowHeader title={t.manualTitle} onBack={() => setStep({ kind: "intro" })} backLabel={t.back} />
        <Panel>
          <Section>
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
            {step.error && <Note tone="danger">{step.error}</Note>}
          </Section>
          <PanelActions>
            <button className="primary" disabled={!manualToken.trim() || !email.includes("@")} onClick={loginManual}>
              {t.continue}
            </button>
          </PanelActions>
        </Panel>
      </div>
    );
  }

  if (step.kind === "list") {
    const { backups, refused } = step.login;
    const busy = job?.status === "running";
    const existing = stores.find((s) => s.sourceApp === source);
    const needsKey = mode !== "folder";
    return (
      <div className="flow">
        <FlowHeader
          title={t.listTitle}
          subtitle={t.listSubtitle(backups.length)}
          onBack={onBack}
          backLabel={back}
          actions={
            <button className="small-btn" onClick={signOut}>
              <Icon name="logOut" size={14} /> {t.signOut}
            </button>
          }
        />
        <Panel>
          <Section label={t.sectionBackup}>
            {backups.length === 0 && <Note>{t.noBackups}</Note>}
            <div className="option-list">
              {backups.map((b) => (
                <OptionCard
                  key={b.id}
                  name="drive-backup"
                  selected={selected?.id === b.id}
                  onSelect={() => pick(b)}
                  icon="cloud"
                  title={
                    <>
                      {b.updateTime ? formatDateTime(new Date(b.updateTime).getTime(), lang) : t.backup}
                      {b.phone && <span className="muted">{formatPhone(b.phone)}</span>}
                      {b.encrypted === true && (
                        <span className="tag">
                          <Icon name="lock" size={11} /> {t.endToEnd}
                        </span>
                      )}
                      {b.encrypted === false && <span className="tag warn">{t.noE2eTag}</span>}
                      {b.uploading && <span className="tag warn">{t.maybeIncomplete}</span>}
                    </>
                  }
                  hint={[
                    b.messages != null && t.messagesCount(formatCount(b.messages, lang)),
                    b.chatDbSize != null && t.chatDbSize(formatSize(b.chatDbSize, lang)),
                    b.sizeBytes != null && t.totalSize(formatSize(b.sizeBytes, lang)),
                  ]
                    .filter(Boolean)
                    .join(" · ")}
                />
              ))}
            </div>
            {selected?.uploading && <Note tone="warn">{t.uploadingNote}</Note>}
            {selected?.encrypted === false && (
              <Note tone="warn">
                <p>
                  <strong>{t.noE2eTitle}</strong>
                  {t.noE2eBefore}
                  <code>key</code>
                  {t.noE2eAfter}
                </p>
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
                <p>{t.noE2eBusiness}</p>
              </Note>
            )}
            {refused.length > 0 && backups.length > 0 && (
              <p className="muted small">{t.refused(refused.map((a) => SOURCE_LABEL[a]).join(t.and))}</p>
            )}
          </Section>

          {selected && (
            <>
              <Section label={t.importAs}>
                <SourcePicker value={source} onChange={setSource} />
                {selected.apps.length > 1 && <p className="muted small">{t.importAsHint}</p>}
              </Section>

              <Section label={t.whatToDo}>
                {folderOnly ? (
                  <Note icon="folder">{t.folderOnlyNote}</Note>
                ) : (
                  <div className="option-list">
                    {MODES.map((m) => (
                      <OptionCard
                        key={m}
                        name="drive-mode"
                        selected={mode === m}
                        onSelect={() => setMode(m)}
                        icon={MODE_ICON[m]}
                        title={t.modeTitle[m]}
                        hint={t.modeHint[m]}
                      />
                    ))}
                  </div>
                )}
                <SwitchRow
                  checked={includeMedia}
                  onChange={setIncludeMedia}
                  icon="image"
                  label={t.mediaSwitch}
                  detail={selected.mediaSize != null ? t.mediaSwitchSize(formatSize(selected.mediaSize, lang)) : t.mediaSwitchDetail}
                />
                {mode !== "vault" && <Note icon="folder">{t.saveFolderHint}</Note>}
              </Section>

              {needsKey && (
                <Section label={t.keyLegend}>
                  <KeyField version={selected.encrypted === false ? "crypt14" : "crypt15"} value={key} onChange={setKey} />
                  {existing && <ReplaceWarning source={source} at={existing.importedAtMs} />}
                </Section>
              )}
            </>
          )}

          <PanelActions info={selected && includeMedia && needsKey ? t.includeMediaHint : undefined}>
            <button className="primary" disabled={!selected || busy || (needsKey && !keyReady(key))} onClick={download}>
              {t.modeButton[mode]}
            </button>
          </PanelActions>
        </Panel>
      </div>
    );
  }

  return (
    <div className="flow">
      <FlowHeader title={t.driveDownload} subtitle={t.driveIntroSubtitle} onBack={onBack} backLabel={back} />
      <Panel>
        <Section label={t.emailLabel}>
          <input
            type="email"
            autoComplete="email"
            spellCheck={false}
            placeholder="nome@gmail.com"
            value={email}
            onChange={(e) => setEmail(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && email.includes("@") && void login()}
          />
          <span className="muted small">{t.emailHint}</span>
          {step.error && <Note tone="danger">{step.error}</Note>}
          <Note tone="ok" icon="shield">
            {t.introPassword}
          </Note>
          <More summary={t.howItWorks}>
            <ul>
              <li>
                {t.introUnofficialBefore}
                <strong>{t.introUnofficial}</strong>
                {t.introUnofficialAfter}
              </li>
              <li>{t.introMemory}</li>
            </ul>
          </More>
        </Section>
        <PanelActions
          info={
            <>
              {t.refusedWindow}{" "}
              <button className="link" disabled={!email.includes("@")} onClick={() => setStep({ kind: "manual" })}>
                {t.manualLogin}
              </button>
            </>
          }
        >
          <button className="primary" disabled={!email.includes("@")} onClick={login}>
            {t.signIn}
          </button>
        </PanelActions>
      </Panel>
    </div>
  );
}
