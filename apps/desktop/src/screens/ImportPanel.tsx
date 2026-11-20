import { useEffect, useState } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";
import { formatDateTime, Icon, useLang } from "@wk/viewer";
import { api, BackupInfo, formatSize, SOURCE_LABEL, SourceApp, StoreInfo } from "../api";
import { useErrorText } from "../i18n/errors";
import { useImportT } from "../i18n/import";
import { useJob } from "../jobs";
import { useVaultGate } from "../vault";
import { initialKey, KeyField, KeyState, keyReady, SourcePicker, toKeyInput } from "./shared";
import { FlowHeader, More, Note, Panel, PanelActions, Section, SwitchRow } from "./ui";

type Step = { kind: "pick"; error?: string } | { kind: "configure"; path: string; info: BackupInfo };

const fileName = (path: string) => path.split(/[\\/]/).pop() ?? path;

type Props = { stores: StoreInfo[]; onBack: () => void };

/**
 * Import of a backup file: copied from the phone, or a backup folder saved
 * from Google Drive (then its media can come in too). Runs as an app-wide job.
 */
export default function ImportPanel({ stores, onBack }: Props) {
  const [step, setStep] = useState<Step>({ kind: "pick" });
  const [source, setSource] = useState<SourceApp>("whatsapp");
  const [key, setKey] = useState<KeyState>(initialKey("crypt15"));
  const [withMedia, setWithMedia] = useState(true);
  const t = useImportT();
  const { lang } = useLang();
  const errorText = useErrorText();
  const { job, start } = useJob();
  const { requireVault } = useVaultGate();
  const busy = job?.status === "running";

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

  function runImport() {
    if (step.kind !== "configure") return;
    const input = toKeyInput(key);
    const { path, info } = step;
    const media = withMedia && info.mediaFolder;
    setKey((k) => ({ ...k, hex: "" })); // the key doesn't stay in React state after being sent
    const target = source;
    requireVault(() => {
      setStep({ kind: "pick" });
      start("local", target, () => api.importBackup(path, input, target, media), {
        steps: media ? ["import", "media"] : ["import"],
      });
    });
  }

  if (step.kind === "pick") {
    return (
      <div className="flow">
        <FlowHeader title={t.hubFileTitle} subtitle={t.fileSubtitle} onBack={onBack} backLabel={t.back} />
        <Panel>
          <Section>
            <div className="dropzone">
              <Icon name="import" size={36} className="dropzone-icon" />
              <p className="title">{t.dropTitle}</p>
              <p className="subtitle">{t.dropSubtitle}</p>
              <button className="primary" onClick={chooseBackup}>
                {t.chooseFile}
              </button>
            </div>
            {step.error && <Note tone="danger">{step.error}</Note>}
            <More summary={t.whereIsTheBackup}>
              <ul>
                <li>{t.whereDrive}</li>
                <li>{t.wherePhone}</li>
              </ul>
            </More>
          </Section>
        </Panel>
      </div>
    );
  }

  const existing = stores.find((s) => s.sourceApp === source);

  return (
    <div className="flow">
      <FlowHeader title={t.hubFileTitle} subtitle={t.fileSubtitle} onBack={onBack} backLabel={t.back} />
      <Panel>
        <Section label={t.sectionFile}>
          <div className="file-pick">
            <Icon name="document" size={22} />
            <span className="file-pick-name">
              <strong>{fileName(step.path)}</strong>
              <span className="muted small">
                {step.info.version} · {formatSize(step.info.sizeBytes, lang)}
              </span>
            </span>
            <button className="small-btn" onClick={() => setStep({ kind: "pick" })}>
              {t.change}
            </button>
          </div>
          {step.info.mediaFolder && (
            <SwitchRow
              checked={withMedia}
              onChange={setWithMedia}
              icon="image"
              label={t.withFolderMedia}
              detail={t.withFolderMediaDetail}
            />
          )}
        </Section>
        <Section label={t.importAs}>
          <SourcePicker value={source} onChange={setSource} />
        </Section>
        <Section label={t.keyLegend}>
          <KeyField version={step.info.version} value={key} onChange={setKey} />
          {existing && <ReplaceWarning source={source} at={existing.importedAtMs} />}
        </Section>
        <PanelActions>
          <button className="primary" disabled={!keyReady(key) || busy} onClick={runImport}>
            {t.import}
          </button>
        </PanelActions>
      </Panel>
    </div>
  );
}

export function ReplaceWarning({ source, at }: { source: SourceApp; at: number }) {
  const t = useImportT();
  const { lang } = useLang();
  return <Note tone="warn">{t.replaceWarning(SOURCE_LABEL[source], formatDateTime(at, lang))}</Note>;
}
