// Long imports and downloads run as one app-wide job, so they keep going (and
// keep reporting progress) while the user reads the history. The backend
// allows a single job at a time.
import { createContext, ReactNode, useCallback, useContext, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { formatCount, formatSize, Icon, IconName, useLang } from "@wk/viewer";
import {
  DownloadProgress,
  driveApi,
  DownloadSummary,
  ImportPhase,
  ImportSummary,
  mediaApi,
  MediaProgress,
  MediaSummary,
  SOURCE_LABEL,
  SourceApp,
} from "./api";
import { useImportT } from "./i18n/import";
import { stoppedText, useFriendlyError } from "./screens/shared";
import { Badge, More, Note, Panel, PanelActions, Section } from "./screens/ui";

export type JobKind = "drive" | "local" | "media";

/**
 * What a job does, in order, as the progress screen lists it:
 * `download` the message database · `import` it (decrypt + index) ·
 * `files` the rest of the backup into a folder · `media` from a folder into
 * the vault · `mediaDownload` from Drive straight into the vault.
 */
export type JobStep = "download" | "import" | "files" | "media" | "mediaDownload";

export type JobResult = {
  folder?: string | null;
  download?: DownloadSummary | null;
  summary?: ImportSummary | null;
  media?: MediaSummary | null;
};

type Stage = "starting" | "database" | "reading" | "decrypting" | "importing" | "files" | "media";

type Running = {
  status: "running";
  kind: JobKind;
  source: SourceApp;
  steps: JobStep[];
  stage: Stage;
  download: DownloadProgress | null;
  media: MediaProgress | null;
  /** The messages are already in the vault (the rest keeps going). */
  messagesReady: boolean;
  /** Starts with a folder picker (saving a copy, or importing media from a folder). */
  picksFolder: boolean;
};

export type Job =
  | Running
  | { status: "done"; kind: JobKind; source: SourceApp; result: JobResult }
  // The raw error: the text follows language changes.
  | { status: "error"; kind: JobKind; source: SourceApp; error: unknown };

type StartOptions = { steps: JobStep[]; picksFolder?: boolean };

type JobApi = {
  job: Job | null;
  /** Starts a job. `run` resolves to `null` when the user cancelled a picker (nothing happened). */
  start: (kind: JobKind, source: SourceApp, run: () => Promise<JobResult | null>, options: StartOptions) => void;
  cancel: () => void;
  dismiss: () => void;
};

const JobContext = createContext<JobApi | null>(null);

export function useJob(): JobApi {
  const ctx = useContext(JobContext);
  if (!ctx) throw new Error("useJob outside JobProvider");
  return ctx;
}

const STAGE_OF: Partial<Record<ImportPhase, Stage>> = {
  downloading: "database",
  reading: "reading",
  decrypting: "decrypting",
  importing: "importing",
};

/** Holds the job and its progress events. `onStoresChanged` runs as soon as the messages are imported. */
export function JobProvider({ onStoresChanged, children }: { onStoresChanged: () => void; children: ReactNode }) {
  const [job, setJob] = useState<Job | null>(null);
  const storesChanged = useRef(onStoresChanged);
  storesChanged.current = onStoresChanged;

  useEffect(() => {
    const update = (f: (j: Running) => Job) => setJob((j) => (j?.status === "running" ? f(j) : j));
    const unlisteners = [
      listen<{ phase: ImportPhase }>("import-progress", (e) => {
        if (e.payload.phase === "done") {
          update((j) => ({ ...j, messagesReady: true }));
          storesChanged.current();
          return;
        }
        const stage = STAGE_OF[e.payload.phase];
        if (stage) update((j) => ({ ...j, stage }));
      }),
      listen<DownloadProgress>("gdrive-progress", (e) =>
        update((j) => ({ ...j, stage: e.payload.phase === "database" ? "database" : "files", download: e.payload })),
      ),
      listen<MediaProgress>("media-progress", (e) => update((j) => ({ ...j, stage: "media", media: e.payload }))),
    ];
    return () => unlisteners.forEach((u) => void u.then((fn) => fn()));
  }, []);

  const start = useCallback(
    (kind: JobKind, source: SourceApp, run: () => Promise<JobResult | null>, { steps, picksFolder = false }: StartOptions) => {
      setJob({
        status: "running",
        kind,
        source,
        steps,
        stage: "starting",
        download: null,
        media: null,
        messagesReady: false,
        picksFolder,
      });
      run().then(
        (result) => {
          setJob(result ? { status: "done", kind, source, result } : null);
          if (result) storesChanged.current();
        },
        (error) => setJob({ status: "error", kind, source, error }),
      );
    },
    [],
  );

  const cancel = useCallback(() => {
    void mediaApi.cancel();
    if (job?.kind === "drive") void driveApi.cancel();
  }, [job?.kind]);

  const dismiss = useCallback(() => setJob((j) => (j?.status === "running" ? j : null)), []);

  return <JobContext.Provider value={{ job, start, cancel, dismiss }}>{children}</JobContext.Provider>;
}

/** The step a stage belongs to (`starting` is the first step). */
function currentStep(job: Running): JobStep {
  switch (job.stage) {
    case "starting":
      return job.steps[0];
    case "database":
      return "download";
    case "reading":
    case "decrypting":
    case "importing":
      return "import";
    case "files":
      return "files";
    case "media":
      return job.steps.includes("mediaDownload") ? "mediaDownload" : "media";
  }
}

const STEP_ORDER: JobStep[] = ["download", "import", "files", "media", "mediaDownload"];

/** Stages the backend can stop (the database import itself cannot be interrupted). */
const stoppable = (stage: Stage) => stage === "database" || stage === "files" || stage === "media";

const pct = (done: number, total: number) => (total > 0 ? Math.min(100, (done / total) * 100) : 0);

function Bar({ value }: { value: number | null }) {
  return (
    <div className={value === null ? "bar indeterminate" : "bar"}>
      <div style={value === null ? undefined : { width: `${value}%` }} />
    </div>
  );
}

/** What the active step is doing right now: a bar and its numbers. */
function StepDetail({ job, step }: { job: Running; step: JobStep }) {
  const t = useImportT();
  const { lang } = useLang();
  const n = (v: number) => formatCount(v, lang);
  const size = (v: number) => formatSize(v, lang);

  if (job.stage === "starting") {
    return <span>{job.picksFolder ? (job.kind === "media" ? t.mediaFolderTitle : t.jobStarting) : t.phase.downloading}</span>;
  }
  if (step === "import") {
    return (
      <>
        <Bar value={null} />
        <span>{job.stage === "importing" ? t.phase.importing : job.stage === "decrypting" ? t.phase.decrypting : t.phase.reading}</span>
      </>
    );
  }
  const d = job.download;
  if (step === "download" && d) {
    return (
      <>
        <Bar value={pct(d.bytes, d.totalBytes)} />
        <span>{`${size(d.bytes)}${t.ofTotal(size(d.totalBytes))}`}</span>
      </>
    );
  }
  if (step === "files" && d) {
    return (
      <>
        <Bar value={pct(d.bytes, d.totalBytes)} />
        <span>
          {t.filesProgress(n(d.filesDone), n(d.filesTotal), size(d.bytes), size(d.totalBytes))}
          {d.failed > 0 && t.mediaFailed(n(d.failed))}
        </span>
      </>
    );
  }
  const m = job.media;
  if ((step === "media" || step === "mediaDownload") && m) {
    return (
      <>
        <Bar value={pct(m.bytes, m.totalBytes)} />
        <span>{t.mediaProgress(n(m.done), n(m.total), size(m.bytes), size(m.totalBytes), n(m.added))}</span>
      </>
    );
  }
  return <Bar value={null} />;
}

function Progress({ job, onViewChats }: { job: Running; onViewChats?: () => void }) {
  const t = useImportT();
  const { cancel } = useJob();
  const active = currentStep(job);
  // The first listed step at or after the current one (robust to a stage the list does not name).
  const activeIndex = Math.max(0, job.steps.findIndex((s) => STEP_ORDER.indexOf(s) >= STEP_ORDER.indexOf(active)));
  const icon: IconName = job.kind === "drive" ? "cloud" : job.kind === "media" ? "image" : "import";
  const title = job.kind === "drive" ? t.jobTitleDrive : job.kind === "media" ? t.jobTitleMedia : t.jobTitleLocal;

  return (
    <Panel>
      <Section>
        <div className="job-head">
          <Badge icon={icon} />
          <div>
            <h2>{title}</h2>
            <p className="muted small">{SOURCE_LABEL[job.source]}</p>
          </div>
        </div>
      </Section>
      <Section>
        <ol className="steps-list">
          {job.steps.map((step, i) => {
            const state = i < activeIndex ? "done" : i === activeIndex ? "active" : "pending";
            return (
              <li key={step} className={`step ${state}`}>
                <span className="step-dot">{state === "done" && <Icon name="check" size={14} strokeWidth={3} />}</span>
                <span className="step-title">{t.stepLabel[step]}</span>
                {state === "active" && (
                  <div className="step-detail">
                    <StepDetail job={job} step={step} />
                  </div>
                )}
              </li>
            );
          })}
        </ol>
        {job.messagesReady && (
          <Note tone="ok" icon="chats">
            <p>{t.messagesReady(SOURCE_LABEL[job.source])}</p>
          </Note>
        )}
        {job.kind === "drive" && (job.stage === "files" || active === "mediaDownload") && (
          <p className="muted small">{t.resumeHint}</p>
        )}
      </Section>
      {(stoppable(job.stage) || (job.messagesReady && onViewChats)) && (
        <PanelActions>
          {stoppable(job.stage) && (
            <button className="ghost-danger" onClick={cancel}>
              {t.stop}
            </button>
          )}
          {job.messagesReady && onViewChats && (
            <button className="primary" onClick={onViewChats}>
              {t.viewChats}
            </button>
          )}
        </PanelActions>
      )}
    </Panel>
  );
}

function Result({ job, onViewChats }: { job: Extract<Job, { status: "done" }>; onViewChats?: () => void }) {
  const t = useImportT();
  const { lang } = useLang();
  const { dismiss } = useJob();
  const n = (v: number) => formatCount(v, lang);
  const { folder, download, summary, media } = job.result;
  const partial = Boolean(download?.stopped || media?.stopped || (download?.failed ?? 0) > 0 || (media?.failed ?? 0) > 0);
  const stopped = stoppedText(t, download?.stopped ?? media?.stopped ?? null);

  return (
    <Panel>
      <Section>
        <div className="job-head">
          <Badge icon={partial ? "warning" : "checkCircle"} tone={partial ? "warn" : "ok"} />
          <div>
            <h2>{partial ? t.donePartial : summary || media ? t.importDone : t.backupSaved}</h2>
            <p className="muted small">{SOURCE_LABEL[job.source]}</p>
          </div>
        </div>
        {summary ? (
          <div className="stats">
            <div className="stat">
              <strong>{n(summary.chats)}</strong>
              <span>{t.statChats}</span>
            </div>
            <div className="stat">
              <strong>{n(summary.messages)}</strong>
              <span>{t.statMessages}</span>
            </div>
            <div className="stat">
              <strong>{n(media ? media.added + media.already : summary.media)}</strong>
              <span>{media ? t.statMediaInVault : t.statMedia}</span>
            </div>
          </div>
        ) : (
          download && (
            <div className="stats">
              <div className="stat">
                <strong>{n(download.downloaded + download.already)}</strong>
                <span>{t.statFiles}</span>
              </div>
              <div className="stat">
                <strong>{formatSize(download.bytes, lang)}</strong>
                <span>{t.statSaved}</span>
              </div>
            </div>
          )
        )}
        {stopped && (
          <Note tone="warn">
            <p>{download?.stopped ? t.downloadStopped(stopped) : t.stoppedEarly(stopped)}</p>
          </Note>
        )}
      </Section>

      {(download || media || summary) && (
        <Section>
          <div className="facts">
            {download && folder && (
              <div className="fact">
                <Icon name="folder" size={20} />
                <span className="fact-title">{t.backupCopy}</span>
                <span className="fact-detail">
                  <span className="mono-path">{folder}</span>
                  <br />
                  {t.downloadDone(n(download.downloaded), n(download.already))}
                  {download.failed > 0 && t.mediaFailed(n(download.failed))}
                </span>
              </div>
            )}
            {media && (
              <div className="fact">
                <Icon name="image" size={20} />
                <span className="fact-title">{t.media}</span>
                <span className="fact-detail">
                  {t.mediaAdded(n(media.added), n(media.already))}
                  {media.failed > 0 && t.mediaFailed(n(media.failed))}
                  {" · "}
                  {t.mediaMatched(n(media.matched), n(media.expected))}
                </span>
              </div>
            )}
            {summary && (
              <div className="fact">
                <Icon name="chats" size={20} />
                <span className="fact-title">{t.alsoImported}</span>
                <span className="fact-detail">{t.summaryExtra(n(summary.reactions), n(summary.vcards), n(summary.skippedStatus))}</span>
              </div>
            )}
          </div>
          {summary && summary.unknownTypes.length > 0 && (
            <More summary={t.unknownTypesSummary(summary.unknownTypes.length, n(summary.unknownTypes.reduce((a, [, c]) => a + c, 0)))}>
              <p>{t.unknownTypes(summary.unknownTypes.map(([type, count]) => `${type} (${n(count)})`).join(", "))}</p>
            </More>
          )}
          {job.kind === "drive" && (
            <p className="muted small">
              <Icon name="shield" size={14} /> {t.driveFooter}
            </p>
          )}
        </Section>
      )}

      <PanelActions>
        <button onClick={dismiss}>{t.close}</button>
        {onViewChats && (summary || media) && (
          <button className="primary" onClick={onViewChats}>
            {t.viewChats}
          </button>
        )}
      </PanelActions>
    </Panel>
  );
}

/** The job in full: progress, result or error. Shown on the import screen. */
export function JobCard({ onViewChats }: { onViewChats?: () => void }) {
  const { job, dismiss } = useJob();
  const t = useImportT();
  const friendlyError = useFriendlyError();
  if (!job) return null;

  if (job.status === "error") {
    return (
      <Panel>
        <Section>
          <div className="job-head">
            <Badge icon="alert" tone="danger" />
            <div>
              <h2>{t.jobFailed}</h2>
              <p className="muted small">{SOURCE_LABEL[job.source]}</p>
            </div>
          </div>
          <Note tone="danger">
            <p>{friendlyError(job.error)}</p>
          </Note>
        </Section>
        <PanelActions>
          <button onClick={dismiss}>{t.back}</button>
        </PanelActions>
      </Panel>
    );
  }
  if (job.status === "done") return <Result job={job} onViewChats={onViewChats} />;
  return <Progress job={job} onViewChats={onViewChats} />;
}

/** Compact version above the chat list, while the user reads the history. */
export function JobBanner({ onDetails }: { onDetails: () => void }) {
  const { job, cancel, dismiss } = useJob();
  const t = useImportT();
  const { lang } = useLang();
  if (!job) return null;

  if (job.status !== "running") {
    return (
      <div className={`job-banner ${job.status === "error" ? "error" : ""}`}>
        <span>{job.status === "error" ? t.jobFailed : t.jobFinished}</span>
        <button className="link" onClick={onDetails}>
          {t.details}
        </button>
        <button className="icon-btn tiny" title={t.close} onClick={dismiss}>
          <Icon name="close" size={14} />
        </button>
      </div>
    );
  }

  const step = currentStep(job);
  const m = job.stage === "media" ? job.media : null;
  const d = job.stage === "files" || job.stage === "database" ? job.download : null;
  const value = m ? pct(m.bytes, m.totalBytes) : d ? pct(d.bytes, d.totalBytes) : null;
  const count = m
    ? t.countOf(formatCount(m.done, lang), formatCount(m.total, lang))
    : d && job.stage === "files"
      ? t.countOf(formatCount(d.filesDone, lang), formatCount(d.filesTotal, lang))
      : null;
  return (
    <div className="job-banner">
      <div className="spinner small" />
      <button className="link job-banner-text" onClick={onDetails}>
        {t.stepLabel[step]}
        {count && <span className="muted small"> {count}</span>}
      </button>
      {stoppable(job.stage) ? (
        <button className="icon-btn tiny" title={t.stop} onClick={cancel}>
          <Icon name="close" size={14} />
        </button>
      ) : (
        <span />
      )}
      <Bar value={value} />
    </div>
  );
}
