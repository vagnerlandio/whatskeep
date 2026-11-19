import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";
import {
  api,
  BackupInfo,
  errorOf,
  formatCount,
  formatSize,
  ImportPhase,
  ImportSummary,
  KeyInput,
  SOURCE_LABEL,
  SourceApp,
  StoreInfo,
} from "../api";

type Step =
  | { kind: "pick"; error?: string }
  | { kind: "configure"; path: string; info: BackupInfo; error?: string }
  | { kind: "running"; phase: ImportPhase }
  | { kind: "done"; source: SourceApp; summary: ImportSummary };

const PHASE_LABEL: Record<ImportPhase, string> = {
  reading: "Lendo o arquivo…",
  decrypting: "Descriptografando…",
  importing: "Importando mensagens e montando o índice de busca…",
  done: "Concluído",
};

const fileName = (path: string) => path.split(/[\\/]/).pop() ?? path;

/** Dígitos hex da chave, ignorando espaços e hífens. `null` se houver outro caractere. */
function hexDigits(value: string): number | null {
  const clean = value.replace(/[\s-]/g, "");
  return /^[0-9a-fA-F]*$/.test(clean) ? clean.length : null;
}

type Props = { stores: StoreInfo[]; onImported: () => void };

export default function ImportPanel({ stores, onImported }: Props) {
  const [step, setStep] = useState<Step>({ kind: "pick" });
  const [source, setSource] = useState<SourceApp>("whatsapp");
  const [keyMode, setKeyMode] = useState<"hex" | "file">("hex");
  const [hex, setHex] = useState("");
  const [keyFile, setKeyFile] = useState<string | null>(null);

  async function selectBackup(path: string) {
    try {
      const info = await api.inspectBackup(path);
      setKeyMode(info.version === "crypt15" ? "hex" : "file");
      setKeyFile(null);
      setHex("");
      setSource(/business|w4b/i.test(path) ? "business" : "whatsapp");
      setStep({ kind: "configure", path, info });
    } catch (e) {
      setStep({ kind: "pick", error: `${fileName(path)}: ${errorOf(e).message}` });
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
      filters: [{ name: "Backup do WhatsApp", extensions: ["crypt14", "crypt15"] }],
    });
    if (path) await selectBackup(path);
  }

  async function chooseKeyFile() {
    const path = await open({ multiple: false, directory: false });
    if (path) setKeyFile(path);
  }

  async function runImport() {
    if (step.kind !== "configure") return;
    const key: KeyInput = keyMode === "hex" ? { type: "hex", value: hex } : { type: "file", path: keyFile! };
    const configured = step;
    setHex(""); // a chave não fica no estado do React depois de enviada
    setStep({ kind: "running", phase: "reading" });
    const unlisten = await listen<{ phase: ImportPhase }>("import-progress", (e) =>
      setStep((s) => (s.kind === "running" ? { kind: "running", phase: e.payload.phase } : s)),
    );
    try {
      const summary = await api.importBackup(configured.path, key, source);
      setStep({ kind: "done", source, summary });
      onImported();
    } catch (e) {
      const { code, message } = errorOf(e);
      const friendly: Record<string, string> = {
        wrong_key: "A chave não abre este backup. Confira os 64 dígitos ou o arquivo de chave.",
        key_mismatch: "Este arquivo de chave é de outro formato de backup.",
        not_msgstore: "O arquivo foi descriptografado, mas não é um banco de mensagens (msgstore).",
      };
      setStep({ ...configured, error: friendly[code] ?? message });
    } finally {
      unlisten();
    }
  }

  if (step.kind === "running") {
    return (
      <section className="card">
        <h2>Importando</h2>
        <div className="progress">
          <div className="spinner" />
          <span>{PHASE_LABEL[step.phase]}</span>
        </div>
        <p className="muted small">O conteúdo descriptografado fica só na memória e vai direto para o cofre.</p>
      </section>
    );
  }

  if (step.kind === "done") {
    const s = step.summary;
    return (
      <section className="card">
        <h2>Importação concluída</h2>
        <div className="result">
          <strong>{SOURCE_LABEL[step.source]}</strong>
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
              {s.unknownTypes.map(([t, n]) => `${t} (${formatCount(n)})`).join(", ")}. Elas aparecem como
              "mensagem não suportada".
            </p>
          )}
        </div>
        <button onClick={() => setStep({ kind: "pick" })}>Importar outro backup</button>
      </section>
    );
  }

  if (step.kind === "pick") {
    return (
      <section className="card">
        <h2>Importar backup</h2>
        <div className="dropzone">
          <p className="title">Arraste o msgstore.db.crypt15 ou .crypt14 para cá</p>
          <p className="subtitle">No celular fica em Android/media/com.whatsapp/WhatsApp/Databases</p>
          <button onClick={chooseBackup}>Escolher arquivo…</button>
        </div>
        {step.error && <div className="result error">{step.error}</div>}
      </section>
    );
  }

  const digits = hexDigits(hex);
  const existing = stores.find((s) => s.sourceApp === source);
  const keyReady = keyMode === "hex" ? digits === 64 : keyFile !== null;

  return (
    <section className="card">
      <h2>Importar backup</h2>
      <div className="file-row">
        <div>
          <strong>{fileName(step.path)}</strong>
          <span className="muted small">
            {step.info.version} · {formatSize(step.info.sizeBytes)}
          </span>
        </div>
        <button onClick={() => setStep({ kind: "pick" })}>Trocar</button>
      </div>

      <fieldset>
        <legend>De qual app é este backup?</legend>
        {(["whatsapp", "business"] as const).map((app) => (
          <label key={app} className="radio">
            <input type="radio" name="source" checked={source === app} onChange={() => setSource(app)} />
            {SOURCE_LABEL[app]}
          </label>
        ))}
      </fieldset>

      <fieldset>
        <legend>Chave do backup</legend>
        {step.info.version === "crypt15" ? (
          <div className="tabs">
            <button className={keyMode === "hex" ? "active" : ""} onClick={() => setKeyMode("hex")}>
              64 dígitos
            </button>
            <button className={keyMode === "file" ? "active" : ""} onClick={() => setKeyMode("file")}>
              Arquivo encrypted_backup.key
            </button>
          </div>
        ) : (
          <p className="muted small">
            Backups crypt14 não têm criptografia de ponta a ponta: a chave é o arquivo <code>key</code> do celular
            (/data/data/com.whatsapp/files/key, só com root).
          </p>
        )}

        {keyMode === "hex" ? (
          <>
            <textarea
              className="key-input"
              rows={3}
              spellCheck={false}
              autoComplete="off"
              autoCorrect="off"
              autoCapitalize="off"
              placeholder="Cole aqui a chave de 64 dígitos mostrada quando você ativou o backup criptografado"
              value={hex}
              onChange={(e) => setHex(e.target.value)}
            />
            <span className={`hint ${digits === null || (hex && digits !== 64) ? "bad" : ""}`}>
              {digits === null ? "A chave só tem os dígitos 0–9 e as letras a–f." : `${digits}/64 dígitos`}
            </span>
          </>
        ) : (
          <div className="file-row">
            <span className="muted small">{keyFile ? fileName(keyFile) : "Nenhum arquivo escolhido"}</span>
            <button onClick={chooseKeyFile}>Escolher arquivo de chave…</button>
          </div>
        )}
        {step.info.version === "crypt15" && (
          <p className="muted small">
            Se o backup foi protegido só com uma senha (sem a chave de 64 dígitos), não há como abri-lo fora do
            WhatsApp: a chave fica guardada nos servidores dele. No celular, troque a proteção para a chave de 64
            dígitos e faça um backup novo.
          </p>
        )}
      </fieldset>

      {existing && (
        <div className="warning">
          Já existe um histórico do {SOURCE_LABEL[source]} importado em{" "}
          {new Date(existing.importedAtMs).toLocaleString("pt-BR")}. Ele será substituído por este backup.
        </div>
      )}
      {step.error && <div className="result error">{step.error}</div>}

      <button className="primary" disabled={!keyReady} onClick={runImport}>
        Importar
      </button>
    </section>
  );
}
