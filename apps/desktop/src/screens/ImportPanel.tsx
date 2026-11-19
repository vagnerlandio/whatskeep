import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";
import { formatDateTime, useLang } from "@wa/viewer";
import { api, BackupInfo, formatSize, ImportPhase, ImportSummary, SOURCE_LABEL, SourceApp, StoreInfo } from "../api";
import { useErrorText } from "../i18n/errors";
import { useImportT } from "../i18n/import";
import {
  initialKey,
  KeyField,
  KeyState,
  keyReady,
  SourcePicker,
  SummaryView,
  toKeyInput,
  useFriendlyError,
  usePhaseLabel,
} from "./shared";

type Step =
  | { kind: "pick"; error?: string }
  | { kind: "configure"; path: string; info: BackupInfo; error?: string }
  | { kind: "running"; phase: ImportPhase }
  | { kind: "done"; source: SourceApp; summary: ImportSummary };

const fileName = (path: string) => path.split(/[\\/]/).pop() ?? path;

type Props = { stores: StoreInfo[]; onImported: () => void };

/** Import of a backup file copied from the phone. */
export default function ImportPanel({ stores, onImported }: Props) {
  const [step, setStep] = useState<Step>({ kind: "pick" });
  const [source, setSource] = useState<SourceApp>("whatsapp");
  const [key, setKey] = useState<KeyState>(initialKey("crypt15"));
  const t = useImportT();
  const { lang } = useLang();
  const phaseLabel = usePhaseLabel();
  const errorText = useErrorText();
  const friendlyError = useFriendlyError();

  async function selectBackup(path: string) {
    try {
      const info = await api.inspectBackup(path);
      setKey(initialKey(info.version));
      setSource(/business|w4b/i.test(path) ? "business" : "whatsapp");
      setStep({ kind: "configure", path, info });
    } catch (e) {
      setStep({ kind: "pick", error: `${fileName(path)}: ${errorText(e)}` });
    }
  }

  const accepting = step.kind === "pick" || step.kind === "configure";
  useEffect(() => {
    if (!accepting) return;
    const unlisten = getCurrentWebview().onDragDropEvent((event) => {
      if (event.payload.type === "drop" && event.payload.paths[0]) void selectBackup(event.payload.paths[0]);
    });
    return () => void unlisten.then((fn) => fn());
  }, [accepting]);

  async function chooseBackup() {
    const path = await open({
      multiple: false,
      directory: false,
      filters: [{ name: t.backupFilter, extensions: ["crypt14", "crypt15"] }],
    });
    if (path) await selectBackup(path);
  }

  async function runImport() {
    if (step.kind !== "configure") return;
    const input = toKeyInput(key);
    const configured = step;
    setKey((k) => ({ ...k, hex: "" })); // the key doesn't stay in React state after being sent
    setStep({ kind: "running", phase: "reading" });
    const unlisten = await listen<{ phase: ImportPhase }>("import-progress", (e) =>
      setStep((s) => (s.kind === "running" ? { kind: "running", phase: e.payload.phase } : s)),
    );
    try {
      const summary = await api.importBackup(configured.path, input, source);
      setStep({ kind: "done", source, summary });
      onImported();
    } catch (e) {
      setStep({ ...configured, error: friendlyError(e) });
    } finally {
      unlisten();
    }
  }

  if (step.kind === "running") {
    return (
      <section className="card">
        <h2>{t.importing}</h2>
        <div className="progress">
          <div className="spinner" />
          <span>{phaseLabel[step.phase]}</span>
        </div>
        <p className="muted small">{t.inMemoryNote}</p>
      </section>
    );
  }

  if (step.kind === "done") {
    return <SummaryView source={step.source} summary={step.summary} onAgain={() => setStep({ kind: "pick" })} />;
  }

  if (step.kind === "pick") {
    return (
      <section className="card">
        <h2>{t.importFromFile}</h2>
        <div className="dropzone">
          <p className="title">{t.dropTitle}</p>
          <p className="subtitle">{t.dropSubtitle}</p>
          <button onClick={chooseBackup}>{t.chooseFile}</button>
        </div>
        {step.error && <div className="result error">{step.error}</div>}
      </section>
    );
  }

  const existing = stores.find((s) => s.sourceApp === source);

  return (
    <section className="card">
      <h2>{t.importFromFile}</h2>
      <div className="file-row">
        <div>
          <strong>{fileName(step.path)}</strong>
          <span className="muted small">
            {step.info.version} · {formatSize(step.info.sizeBytes, lang)}
          </span>
        </div>
        <button onClick={() => setStep({ kind: "pick" })}>{t.change}</button>
      </div>

      <SourcePicker value={source} onChange={setSource} />
      <KeyField version={step.info.version} value={key} onChange={setKey} />

      {existing && <ReplaceWarning source={source} at={existing.importedAtMs} />}
      {step.error && <div className="result error">{step.error}</div>}

      <button className="primary" disabled={!keyReady(key)} onClick={runImport}>
        {t.import}
      </button>
    </section>
  );
}

export function ReplaceWarning({ source, at }: { source: SourceApp; at: number }) {
  const t = useImportT();
  const { lang } = useLang();
  return <div className="warning">{t.replaceWarning(SOURCE_LABEL[source], formatDateTime(at, lang))}</div>;
}
