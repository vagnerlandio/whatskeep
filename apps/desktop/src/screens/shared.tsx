// Peças compartilhadas pela importação local e pelo download do Google Drive.
import { open } from "@tauri-apps/plugin-dialog";
import { formatCount, formatSize, ImportPhase, ImportSummary, KeyInput, MediaProgress, MediaSummary, SOURCE_LABEL, SourceApp } from "../api";

export type BackupVersion = "crypt14" | "crypt15";
export type KeyState = { mode: "hex" | "file"; hex: string; file: string | null };

export const initialKey = (version: BackupVersion): KeyState => ({
  mode: version === "crypt15" ? "hex" : "file",
  hex: "",
  file: null,
});

/** Dígitos hex da chave, ignorando espaços e hífens. `null` se houver outro caractere. */
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
  const digits = hexDigits(value.hex);

  async function chooseKeyFile() {
    const path = await open({ multiple: false, directory: false });
    if (path) onChange({ ...value, file: path });
  }

  return (
    <fieldset>
      <legend>Chave do backup</legend>
      {version === "crypt15" ? (
        <div className="tabs">
          <button className={value.mode === "hex" ? "active" : ""} onClick={() => onChange({ ...value, mode: "hex" })}>
            64 dígitos
          </button>
          <button className={value.mode === "file" ? "active" : ""} onClick={() => onChange({ ...value, mode: "file" })}>
            Arquivo encrypted_backup.key
          </button>
        </div>
      ) : (
        <p className="muted small">
          Backups crypt14 não têm criptografia de ponta a ponta: a chave é o arquivo <code>key</code> do celular
          (/data/data/com.whatsapp/files/key, só com root).
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
            placeholder="Cole aqui a chave de 64 dígitos mostrada quando você ativou o backup criptografado"
            value={value.hex}
            onChange={(e) => onChange({ ...value, hex: e.target.value })}
          />
          <span className={`hint ${digits === null || (value.hex && digits !== 64) ? "bad" : ""}`}>
            {digits === null ? "A chave só tem os dígitos 0–9 e as letras a–f." : `${digits}/64 dígitos`}
          </span>
        </>
      ) : (
        <div className="file-row">
          <span className="muted small">{value.file ? fileName(value.file) : "Nenhum arquivo escolhido"}</span>
          <button onClick={chooseKeyFile}>Escolher arquivo de chave…</button>
        </div>
      )}
      {version === "crypt15" && (
        <p className="muted small">
          Se o backup foi protegido só com uma senha (sem a chave de 64 dígitos), não há como abri-lo fora do
          WhatsApp: a chave fica guardada nos servidores dele. No celular, troque a proteção para a chave de 64 dígitos
          e faça um backup novo.
        </p>
      )}
    </fieldset>
  );
}

export function SourcePicker({ value, onChange }: { value: SourceApp; onChange: (s: SourceApp) => void }) {
  return (
    <fieldset>
      <legend>Importar como</legend>
      {(["whatsapp", "business"] as const).map((app) => (
        <label key={app} className="radio">
          <input type="radio" checked={value === app} onChange={() => onChange(app)} />
          {SOURCE_LABEL[app]}
        </label>
      ))}
    </fieldset>
  );
}

export const PHASE_LABEL: Record<ImportPhase, string> = {
  reading: "Lendo o arquivo…",
  downloading: "Baixando do Google Drive…",
  decrypting: "Descriptografando…",
  importing: "Importando mensagens e montando o índice de busca…",
  done: "Concluído",
};

/** Mensagens amigáveis para os códigos de erro mais comuns. */
export function friendlyError(code: string, message: string): string {
  const map: Record<string, string> = {
    wrong_key: "A chave não abre este backup. Confira os 64 dígitos ou o arquivo de chave.",
    key_mismatch: "Este arquivo de chave é de outro formato de backup.",
    not_msgstore: "O arquivo foi descriptografado, mas não é um banco de mensagens (msgstore).",
  };
  return map[code] ?? message;
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
  return (
    <section className="card">
      <h2>Importação concluída</h2>
      <div className="result">
        <strong>{SOURCE_LABEL[source]}</strong>
        <span>
          {formatCount(s.chats)} conversas · {formatCount(s.messages)} mensagens · {formatCount(s.media)} mídias
        </span>
        <span className="muted small">
          {formatCount(s.reactions)} reações · {formatCount(s.vcards)} contatos compartilhados ·{" "}
          {formatCount(s.skippedStatus)} status ignorados
        </span>
        {s.unknownTypes.length > 0 && (
          <p>
            Tipos de mensagem ainda não reconhecidos:{" "}
            {s.unknownTypes.map(([t, n]) => `${t} (${formatCount(n)})`).join(", ")}. Elas aparecem como "mensagem não
            suportada".
          </p>
        )}
      </div>
      {media && <MediaSummaryLine s={media} />}
      {footer && <p className="muted small">{footer}</p>}
      <button onClick={onAgain}>Importar outro backup</button>
    </section>
  );
}

export function MediaSummaryLine({ s }: { s: MediaSummary }) {
  return (
    <div className={`result ${s.failed > 0 || s.stopped ? "partial" : ""}`}>
      <strong>Mídias</strong>
      <span>
        {formatCount(s.added)} novas · {formatCount(s.already)} já estavam no cofre
        {s.failed > 0 && ` · ${formatCount(s.failed)} com erro`} · {formatSize(s.bytes)}
      </span>
      <span className="muted small">
        {formatCount(s.matched)} de {formatCount(s.expected)} mídias do histórico encontradas na origem
        {s.unmatched > 0 && ` · ${formatCount(s.unmatched)} arquivos sem mensagem correspondente ignorados`}
      </span>
      {s.stopped && <p>Parou antes do fim: {s.stopped}. O que já entrou continua no cofre; rode de novo para continuar.</p>}
    </div>
  );
}

export function MediaProgressView({ p, onCancel }: { p: MediaProgress | null; onCancel: () => void }) {
  const pct = p && p.totalBytes > 0 ? Math.min(100, (p.bytes / p.totalBytes) * 100) : 0;
  return (
    <>
      <div className="bar">
        <div style={{ width: `${pct}%` }} />
      </div>
      <span className="muted small">
        {p
          ? `${formatCount(p.done)} de ${formatCount(p.total)} arquivos · ${formatSize(p.bytes)} de ${formatSize(p.totalBytes)} · ${formatCount(p.added)} novos`
          : "Procurando os arquivos…"}
      </span>
      <button onClick={onCancel}>Parar</button>
    </>
  );
}
