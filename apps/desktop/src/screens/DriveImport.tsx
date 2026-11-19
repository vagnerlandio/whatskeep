import { Icon } from "@wa/viewer";
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
import { ReplaceWarning } from "./ImportPanel";
import {
  friendlyError,
  initialKey,
  KeyField,
  KeyState,
  keyReady,
  MediaProgressView,
  PHASE_LABEL,
  SourcePicker,
  SummaryView,
  toKeyInput,
} from "./shared";

type Step =
  | { kind: "intro"; error?: string }
  | { kind: "waiting" }
  | { kind: "manual"; error?: string }
  | { kind: "list"; login: DriveLogin; error?: string }
  | { kind: "running"; phase: ImportPhase; downloaded: number; total: number; media: MediaProgress | null }
  | { kind: "done"; source: SourceApp; summary: ImportSummary; media: MediaSummary | null };

type Props = { stores: StoreInfo[]; onImported: () => void };

/** Login no Google e download do backup direto do Google Drive. */
export default function DriveImport({ stores, onImported }: Props) {
  const [step, setStep] = useState<Step>({ kind: "intro" });
  const [email, setEmail] = useState("");
  const [manualToken, setManualToken] = useState("");
  const [selected, setSelected] = useState<DriveBackup | null>(null);
  const [source, setSource] = useState<SourceApp>("whatsapp");
  const [key, setKey] = useState<KeyState>(initialKey("crypt15"));
  const [includeMedia, setIncludeMedia] = useState(false);
  const sessionOpen = useRef(false);

  // Ao sair da tela, os tokens do Google são descartados no backend.
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
      const { code, message } = errorOf(e);
      setStep({ kind: "intro", error: code === "cancelled" ? undefined : message });
    }
  }

  async function loginManual() {
    try {
      loggedIn(await driveApi.loginManual(email.trim(), manualToken));
    } catch (e) {
      setManualToken("");
      setStep({ kind: "manual", error: errorOf(e).message });
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
      const { code, message } = errorOf(e);
      // Rede: a sessão continua no backend e dá para tentar de novo. Outros erros encerram a sessão.
      if (code === "network" || code === "download_corrupt") {
        setStep({ kind: "list", login, error: message });
      } else {
        sessionOpen.current = false;
        setStep({ kind: "intro", error: code === "cancelled" ? undefined : friendlyError(code, message) });
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
        footer="A sessão do Google foi encerrada e os tokens de acesso foram apagados da memória."
        onAgain={() => setStep({ kind: "intro" })}
      />
    );
  }

  if (step.kind === "running") {
    const pct = step.total > 0 ? Math.min(100, (step.downloaded / step.total) * 100) : 0;
    const mediaPhase = step.phase === "done" && includeMedia;
    return (
      <section className="card">
        <h2>Google Drive</h2>
        <div className="progress">
          <div className="spinner" />
          <span>{mediaPhase ? "Baixando fotos, vídeos e áudios…" : PHASE_LABEL[step.phase]}</span>
        </div>
        {step.phase === "downloading" && (
          <>
            <div className="bar">
              <div style={{ width: `${pct}%` }} />
            </div>
            <span className="muted small">
              {formatSize(step.downloaded)}
              {step.total > 0 && ` de ${formatSize(step.total)}`}
            </span>
            <button onClick={cancel}>Cancelar</button>
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
        <p className="muted small">O backup vem cifrado do Google e é aberto só na memória, direto para o cofre.</p>
      </section>
    );
  }

  if (step.kind === "waiting") {
    return (
      <section className="card">
        <h2>Google Drive</h2>
        <div className="progress">
          <div className="spinner" />
          <span>Entre na sua conta na janela do Google que foi aberta.</span>
        </div>
        <p className="muted small">
          Ela fecha sozinha quando o login terminar. Se o Google disser que o navegador não é seguro, cancele e use a
          opção manual.
        </p>
        <button onClick={cancel}>Cancelar</button>
      </section>
    );
  }

  if (step.kind === "manual") {
    return (
      <section className="card">
        <h2>Google Drive: login manual</h2>
        <ol className="steps">
          <li>
            Num navegador, abra <code>https://accounts.google.com/EmbeddedSetup</code> e entre na conta {email || "do backup"}.
          </li>
          <li>
            Quando a página parar de carregar, abra as ferramentas do desenvolvedor → Aplicativo/Armazenamento →
            Cookies → <code>accounts.google.com</code>.
          </li>
          <li>
            Copie o valor do cookie <code>oauth_token</code> (começa com <code>oauth2_4/</code>) e cole abaixo. Ele vale
            por poucos minutos e só uma vez.
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
          <button onClick={() => setStep({ kind: "intro" })}>Voltar</button>
          <button className="primary" disabled={!manualToken.trim() || !email.includes("@")} onClick={loginManual}>
            Continuar
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
          <h2>Backups no Google Drive</h2>
          <button onClick={cancel}>Sair da conta</button>
        </div>
        {backups.length === 0 && (
          <p className="muted">Nenhum backup do WhatsApp encontrado nesta conta.</p>
        )}
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
                  {b.phone ? formatPhone(b.phone) : "Backup"}{" "}
                  {b.apps.map((a) => (
                    <span key={a} className="badge">
                      {SOURCE_LABEL[a]}
                    </span>
                  ))}
                  {b.encrypted && <span className="badge">
                      <Icon name="lock" size={12} /> ponta a ponta
                    </span>}
                </strong>
                <span className="muted small">
                  {b.uploading
                    ? "Ainda sendo enviado pelo celular"
                    : [
                        b.updateTime && new Date(b.updateTime).toLocaleString("pt-BR"),
                        b.chatDbSize != null && `mensagens: ${formatSize(b.chatDbSize)}`,
                        b.messages != null && `${formatCount(b.messages)} mensagens`,
                        b.sizeBytes != null && `backup total: ${formatSize(b.sizeBytes)}`,
                      ]
                        .filter(Boolean)
                        .join(" · ")}
                </span>
              </span>
            </label>
          ))}
        </div>
        {refused.length > 0 && backups.length > 0 && (
          <p className="muted small">
            O Google não liberou acesso pelo {refused.map((a) => SOURCE_LABEL[a]).join(" e ")}. Se você usa esse app,
            os backups dele podem não aparecer aqui.
          </p>
        )}
        {selected && (
          <>
            <SourcePicker value={source} onChange={setSource} />
            {selected.encrypted === false && (
              <div className="warning">
                <strong>Este backup não tem criptografia de ponta a ponta.</strong> A chave dele fica num arquivo interno
                do celular (<code>key</code>), que só dá para copiar com root. Sem root, o caminho é:
                <ol>
                  <li>
                    No WhatsApp: <em>Configurações → Conversas → Backup de conversas → Backup criptografado de ponta a
                    ponta → Ativar</em>.
                  </li>
                  <li>
                    Escolha <em>Usar chave de criptografia de 64 dígitos</em> (não a senha) e anote a chave num lugar
                    seguro.
                  </li>
                  <li>Faça um novo backup e espere terminar de enviar.</li>
                  <li>Volte aqui, entre de novo e use a chave de 64 dígitos.</li>
                </ol>
                No WhatsApp Business o caminho é o mesmo, com uma chave própria.
              </div>
            )}
            <KeyField version={selected.encrypted === false ? "crypt14" : "crypt15"} value={key} onChange={setKey} />
            {existing && <ReplaceWarning source={source} at={existing.importedAtMs} />}
            <label className="radio">
              <input type="checkbox" checked={includeMedia} onChange={(e) => setIncludeMedia(e.target.checked)} />
              Baixar também fotos, vídeos, áudios e documentos
              {selected.mediaSize != null && ` (até ${formatSize(selected.mediaSize)})`}
            </label>
            {includeMedia && (
              <p className="muted small">
                Pode demorar bastante. Cada arquivo entra cifrado no cofre; se parar no meio, rode de novo que ele
                continua de onde estava. O acesso ao Google vale 1 hora.
              </p>
            )}
          </>
        )}
        {step.error && <div className="result error">{step.error}</div>}
        <button className="primary" disabled={!selected || !keyReady(key)} onClick={download}>
          Baixar e importar
        </button>
      </section>
    );
  }

  return (
    <section className="card">
      <h2>Baixar do Google Drive</h2>
      <ul className="muted small bullets">
        <li>
          Usa o mesmo acesso do app do WhatsApp no Android. <strong>Não é um método oficial</strong>: o Google pode
          bloqueá-lo, e você pode receber um alerta de "novo dispositivo" na conta.
        </li>
        <li>
          O acesso fica só na memória deste app, limitado à pasta de backup do WhatsApp, e é apagado ao terminar o
          download, ao travar o cofre ou depois de 50 minutos.
        </li>
        <li>O login acontece numa janela do próprio Google; este app não vê a sua senha.</li>
      </ul>
      <label>
        E-mail da conta Google do backup
        <input
          type="email"
          autoComplete="email"
          spellCheck={false}
          value={email}
          onChange={(e) => setEmail(e.target.value)}
        />
      </label>
      <span className="muted small">A janela do Google já abre com este e-mail preenchido.</span>
      {step.error && <div className="result error">{step.error}</div>}
      <button className="primary" disabled={!email.includes("@")} onClick={login}>
        Entrar no Google
      </button>
      <p className="muted small">
        O Google recusou a janela de login?{" "}
        <button className="link" disabled={!email.includes("@")} onClick={() => setStep({ kind: "manual" })}>
          Entrar pelo navegador (login manual)
        </button>
      </p>
    </section>
  );
}
