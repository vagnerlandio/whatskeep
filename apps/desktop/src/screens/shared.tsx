// Pieces shared by the local import and the Google Drive download.
import { open } from "@tauri-apps/plugin-dialog";
import { useLang } from "@wa/viewer";
import { errorOf, formatCount, formatSize, ImportPhase, ImportSummary, KeyInput, MediaProgress, MediaSummary, SOURCE_LABEL, SourceApp } from "../api";
import { useErrorText } from "../i18n/errors";
import { useImportT } from "../i18n/import";

export type BackupVersion = "crypt14" | "crypt15";
export type KeyState = { mode: "hex" | "file"; hex: string; file: string | null };

export const initialKey = (version: BackupVersion): KeyState => ({
  mode: version === "crypt15" ? "hex" : "file",
  hex: "",
  file: null,
});

/** Hex digits of the key, ignoring spaces and hyphens. `null` if there's any other character. */
function hexDigits(value: string): number | null {
  const clean = value.replace(/[\s-]/g, "");
  return /^[0-9a-fA-F]*$/.test(clean) ? clean.length : null;
}

export const keyReady = (k: KeyState) => (k.mode === "hex" ? hexDigits(k.hex) === 64 : k.file !== null);

export const toKeyInput = (k: KeyState): KeyInput =>
  k.mode === "hex" ? { type: "hex", value: k.hex } : { type: "file", path: k.file! };

const fileName = (path: string) => path.split(/[\\/]/).pop() ?? path;

export function KeyField({
  version,
  value,
  onChange,
}: {
  version: BackupVersion;
  value: KeyState;
  onChange: (k: KeyState) => void;
}) {
  const t = useImportT();
  const digits = hexDigits(value.hex);

  async function chooseKeyFile() {
    const path = await open({ multiple: false, directory: false });
    if (path) onChange({ ...value, file: path });
  }

  return (
    <fieldset>
      <legend>{t.keyLegend}</legend>
      {version === "crypt15" ? (
        <div className="tabs">
          <button className={value.mode === "hex" ? "active" : ""} onClick={() => onChange({ ...value, mode: "hex" })}>
            {t.keyTabHex}
          </button>
          <button className={value.mode === "file" ? "active" : ""} onClick={() => onChange({ ...value, mode: "file" })}>
            {t.keyTabFile}
          </button>
        </div>
      ) : (
        <p className="muted small">
          {t.crypt14KeyBefore}
          <code>key</code>
          {t.crypt14KeyAfter}
        </p>
      )}

      {value.mode === "hex" ? (
        <>
          <textarea
            className="key-input"
            rows={3}
            spellCheck={false}
            autoComplete="off"
            autoCorrect="off"
            autoCapitalize="off"
            placeholder={t.keyPlaceholder}
            value={value.hex}
            onChange={(e) => onChange({ ...value, hex: e.target.value })}
          />
          <span className={`hint ${digits === null || (value.hex && digits !== 64) ? "bad" : ""}`}>
            {digits === null ? t.keyBadChars : t.keyDigits(digits)}
          </span>
        </>
      ) : (
        <div className="file-row">
          <span className="muted small">{value.file ? fileName(value.file) : t.noFileChosen}</span>
          <button onClick={chooseKeyFile}>{t.chooseKeyFile}</button>
        </div>
      )}
      {version === "crypt15" && (
        <p className="muted small">{t.passwordOnly}</p>
      )}
    </fieldset>
  );
}

export function SourcePicker({ value, onChange }: { value: SourceApp; onChange: (s: SourceApp) => void }) {
  const t = useImportT();
  return (
    <fieldset>
      <legend>{t.importAs}</legend>
      {(["whatsapp", "business"] as const).map((app) => (
        <label key={app} className="radio">
          <input type="radio" checked={value === app} onChange={() => onChange(app)} />
          {SOURCE_LABEL[app]}
        </label>
      ))}
    </fieldset>
  );
}

/** Labels of the import phases in the current language. */
export function usePhaseLabel(): Record<ImportPhase, string> {
  return useImportT().phase;
}

/** Friendly messages for the most common error codes; the rest come from `useErrorText`. */
export function useFriendlyError(): (e: unknown) => string {
  const t = useImportT();
  const errorText = useErrorText();
  return (e: unknown) => {
    const code = errorOf(e).code;
    return t.friendly[code] ?? errorText(e);
  };
}

export function SummaryView({
  source,
  summary: s,
  media,
  footer,
  onAgain,
}: {
  source: SourceApp;
  summary: ImportSummary;
  media?: MediaSummary | null;
  footer?: string;
  onAgain: () => void;
}) {
  const t = useImportT();
  const { lang } = useLang();
  const n = (v: number) => formatCount(v, lang);
  return (
    <section className="card">
      <h2>{t.importDone}</h2>
      <div className="result">
        <strong>{SOURCE_LABEL[source]}</strong>
        <span>{t.summaryMain(n(s.chats), n(s.messages), n(s.media))}</span>
        <span className="muted small">{t.summaryExtra(n(s.reactions), n(s.vcards), n(s.skippedStatus))}</span>
        {s.unknownTypes.length > 0 && (
          <p>{t.unknownTypes(s.unknownTypes.map(([type, count]) => `${type} (${n(count)})`).join(", "))}</p>
        )}
      </div>
      {media && <MediaSummaryLine s={media} />}
      {footer && <p className="muted small">{footer}</p>}
      <button onClick={onAgain}>{t.importAnother}</button>
    </section>
  );
}

// Stop reasons the backend sends in Portuguese (see gdrive.rs and media_import.rs).
// Matched exactly: keep in sync with the backend strings.
const STOPPED_CANCELLED = "cancelado";
const STOPPED_EXPIRED = "a sessão do Google expirou; entre de novo para continuar";

export function MediaSummaryLine({ s }: { s: MediaSummary }) {
  const t = useImportT();
  const { lang } = useLang();
  const n = (v: number) => formatCount(v, lang);
  const stopped =
    s.stopped === STOPPED_CANCELLED ? t.stoppedCancelled : s.stopped === STOPPED_EXPIRED ? t.stoppedExpired : s.stopped;
  return (
    <div className={`result ${s.failed > 0 || s.stopped ? "partial" : ""}`}>
      <strong>{t.media}</strong>
      <span>
        {t.mediaAdded(n(s.added), n(s.already))}
        {s.failed > 0 && t.mediaFailed(n(s.failed))} · {formatSize(s.bytes, lang)}
      </span>
      <span className="muted small">
        {t.mediaMatched(n(s.matched), n(s.expected))}
        {s.unmatched > 0 && t.mediaUnmatched(n(s.unmatched))}
      </span>
      {stopped && <p>{t.stoppedEarly(stopped)}</p>}
    </div>
  );
}

export function MediaProgressView({ p, onCancel }: { p: MediaProgress | null; onCancel: () => void }) {
  const t = useImportT();
  const { lang } = useLang();
  const pct = p && p.totalBytes > 0 ? Math.min(100, (p.bytes / p.totalBytes) * 100) : 0;
  return (
    <>
      <div className="bar">
        <div style={{ width: `${pct}%` }} />
      </div>
      <span className="muted small">
        {p
          ? t.mediaProgress(
              formatCount(p.done, lang),
              formatCount(p.total, lang),
              formatSize(p.bytes, lang),
              formatSize(p.totalBytes, lang),
              formatCount(p.added, lang),
            )
          : t.searchingFiles}
      </span>
      <button onClick={onCancel}>{t.stop}</button>
    </>
  );
}
