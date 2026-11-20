// Pieces shared by the local import and the Google Drive download.
import { open } from "@tauri-apps/plugin-dialog";
import { Icon } from "@wk/viewer";
import { errorOf, KeyInput, SOURCE_LABEL, SourceApp } from "../api";
import { useErrorText } from "../i18n/errors";
import { useImportT } from "../i18n/import";
import { More, Note, Segmented } from "./ui";

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

/** The backup key: 64 digits or a key file (crypt15), or the phone's `key` file (crypt14). */
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
  const countClass = digits === null || (value.hex && digits !== 64) ? "bad" : digits === 64 ? "ok" : "";

  async function chooseKeyFile() {
    const path = await open({ multiple: false, directory: false });
    if (path) onChange({ ...value, file: path });
  }

  return (
    <>
      {version === "crypt15" ? (
        <Segmented
          value={value.mode}
          options={[
            { value: "hex", label: t.keyTabHex },
            { value: "file", label: t.keyTabFile },
          ]}
          onChange={(mode) => onChange({ ...value, mode })}
        />
      ) : (
        <Note icon="key">
          <p>
            {t.crypt14KeyBefore}
            <code>key</code>
            {t.crypt14KeyAfter}
          </p>
        </Note>
      )}

      {value.mode === "hex" ? (
        <div className="key-box">
          <textarea
            className="key-input"
            rows={2}
            spellCheck={false}
            autoComplete="off"
            autoCorrect="off"
            autoCapitalize="off"
            placeholder={t.keyPlaceholder}
            value={value.hex}
            onChange={(e) => onChange({ ...value, hex: e.target.value })}
          />
          <span className={`key-count ${countClass}`}>{digits === null ? t.keyBadChars : t.keyDigits(digits)}</span>
        </div>
      ) : (
        <div className="file-pick">
          <Icon name="key" size={20} />
          <span className="file-pick-name">{value.file ? fileName(value.file) : <span className="muted">{t.noFileChosen}</span>}</span>
          <button className="small-btn" onClick={chooseKeyFile}>
            {t.chooseKeyFile}
          </button>
        </div>
      )}
      {version === "crypt15" && (
        <More summary={t.passwordOnlySummary}>
          <p>{t.passwordOnly}</p>
        </More>
      )}
    </>
  );
}

export function SourcePicker({ value, onChange }: { value: SourceApp; onChange: (s: SourceApp) => void }) {
  return (
    <Segmented
      value={value}
      options={(["whatsapp", "business"] as const).map((app) => ({ value: app, label: SOURCE_LABEL[app] }))}
      onChange={onChange}
    />
  );
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

// Stop reasons the backend sends in Portuguese (see backup_folder.rs).
// Matched exactly: keep in sync with the backend strings.
const STOPPED_CANCELLED = "cancelado";
const STOPPED_EXPIRED = "a sessão do Google expirou; entre de novo para continuar";

/** A backend stop reason in the UI language. */
export function stoppedText(t: ReturnType<typeof useImportT>, stopped: string | null): string | null {
  if (stopped === STOPPED_CANCELLED) return t.stoppedCancelled;
  if (stopped === STOPPED_EXPIRED) return t.stoppedExpired;
  return stopped;
}
