import { FormEvent, ReactNode, useEffect, useRef, useState } from "react";
import { errorOf, Icon, RailButton, Viewer } from "@wa/viewer";
import { BundleSource } from "./bundle";
import { loadManifest } from "./loader";

/** Sem interação por esse tempo, a página esquece tudo (recarrega). */
const AUTOLOCK_MS = 15 * 60 * 1000;

export default function App() {
  const [source, setSource] = useState<BundleSource | null>(null);
  const [fatal, setFatal] = useState<string | null>(null);
  const [unlocked, setUnlocked] = useState(false);
  const [version, setVersion] = useState(0);
  const [askMore, setAskMore] = useState(false);
  const folderInput = useRef<HTMLInputElement>(null);

  useEffect(() => {
    loadManifest()
      .then((m) => setSource(new BundleSource(m as ConstructorParameters<typeof BundleSource>[0])))
      .catch(() =>
        setFatal(
          "Não foi possível ler os dados do histórico. Abra o index.html de dentro da pasta exportada (com a pasta data ao lado).",
        ),
      );
  }, []);

  useEffect(() => {
    if (!unlocked) return;
    let timer = window.setTimeout(lock, AUTOLOCK_MS);
    const reset = () => {
      window.clearTimeout(timer);
      timer = window.setTimeout(lock, AUTOLOCK_MS);
    };
    const events = ["pointerdown", "keydown", "wheel"] as const;
    events.forEach((e) => window.addEventListener(e, reset, { passive: true }));
    return () => {
      window.clearTimeout(timer);
      events.forEach((e) => window.removeEventListener(e, reset));
    };
  }, [unlocked]);

  function lock() {
    source?.dispose();
    window.location.reload();
  }

  function onFolder(files: FileList | null) {
    if (!source || !files) return;
    const n = source.setFolder(files);
    if (n === 0) {
      alert("Nenhuma mídia encontrada. Selecione a pasta que contém o index.html deste histórico.");
      return;
    }
    setVersion((v) => v + 1); // remonta o visualizador para carregar as mídias
  }

  if (fatal) return <Centered><div className="result error">{fatal}</div></Centered>;
  if (!source) return null;
  if (!unlocked || askMore) {
    return (
      <Centered>
        <Unlock
          source={source}
          more={askMore}
          onDone={() => {
            setUnlocked(true);
            setAskMore(false);
            setVersion((v) => v + 1);
          }}
          onCancel={askMore ? () => setAskMore(false) : undefined}
        />
      </Centered>
    );
  }

  const needsFolder = source.mediaMode === "encrypted" && !source.hasMediaFolder;
  const railBottom = (
    <>
      <RailButton icon="key" title="Liberar mais conversas com outra senha" onClick={() => setAskMore(true)} />
      {source.mediaMode === "encrypted" && (
        <RailButton
          icon="folder"
          title="Selecionar a pasta do histórico (fotos e vídeos)"
          onClick={() => folderInput.current?.click()}
        />
      )}
      <RailButton icon="lock" title="Travar (esquece tudo o que foi aberto)" onClick={lock} />
      <input
        ref={folderInput}
        type="file"
        hidden
        // Seleciona a pasta inteira: o navegador só lê arquivos que o usuário escolhe.
        {...{ webkitdirectory: "", directory: "" }}
        onChange={(e) => onFolder(e.target.files)}
      />
    </>
  );
  const banner = needsFolder ? (
    <div className="folder-banner">
      <span>Para ver fotos e vídeos, selecione a pasta deste histórico.</span>
      <button onClick={() => folderInput.current?.click()}>Selecionar pasta</button>
    </div>
  ) : null;

  return (
    <Viewer
      key={version}
      source={source}
      title="Histórico"
      railBottom={railBottom}
      banner={banner}
      emptyTitle="Histórico do WhatsApp"
      emptyNote={`${source.unlockedCount} conversas liberadas. Nada sai deste computador: a página não acessa a internet.`}
    />
  );
}

function Centered({ children }: { children: ReactNode }) {
  return (
    <div className="app">
      <main className="content">{children}</main>
    </div>
  );
}

function Unlock({
  source,
  more,
  onDone,
  onCancel,
}: {
  source: BundleSource;
  more: boolean;
  onDone: () => void;
  onCancel?: () => void;
}) {
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function submit(e: FormEvent) {
    e.preventDefault();
    if (!password || busy) return;
    setBusy(true);
    setError(null);
    // Deixa o "Abrindo…" aparecer antes do Argon2id ocupar a página.
    await new Promise((r) => setTimeout(r, 30));
    try {
      const added = await source.unlock(password);
      setPassword("");
      if (added === 0) {
        setError(more ? "Essa senha não liberou nenhuma conversa nova." : "Senha incorreta.");
        setBusy(false);
        return;
      }
      onDone();
    } catch (err) {
      setError(errorOf(err).message);
      setBusy(false);
    }
  }

  return (
    <form className="card narrow" onSubmit={submit}>
      <div className="lock-icon">
        <Icon name="lockKeyhole" size={40} strokeWidth={1.5} />
      </div>
      <h1>{more ? "Outra senha" : "Histórico do WhatsApp"}</h1>
      <p className="muted">
        {more
          ? "Cada senha libera as conversas que foram exportadas com ela."
          : "Este histórico é cifrado. Digite a senha definida na exportação."}
      </p>
      <label>
        Senha
        <input
          type="password"
          autoFocus
          autoComplete="off"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          disabled={busy}
        />
      </label>
      {error && <div className="result error">{error}</div>}
      <div className="row-buttons">
        {onCancel && (
          <button type="button" onClick={onCancel} disabled={busy}>
            Voltar
          </button>
        )}
        <button className="primary" type="submit" disabled={!password || busy}>
          {busy ? "Abrindo…" : "Abrir"}
        </button>
      </div>
    </form>
  );
}
