import { ReactNode, useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, errorOf, StoreInfo, VaultStatus } from "./api";
import CreateVault from "./screens/CreateVault";
import Home from "./screens/Home";
import Unlock from "./screens/Unlock";
import Viewer from "./viewer/Viewer";
import "./App.css";
import "./viewer/viewer.css";

/** Intervalo mínimo entre avisos de atividade ao backend (autolock). */
const TOUCH_EVERY_MS = 15_000;

export default function App() {
  const [status, setStatus] = useState<VaultStatus | null>(null);
  const [lockReason, setLockReason] = useState<string | null>(null);

  const refresh = useCallback(async () => setStatus(await api.status()), []);

  useEffect(() => {
    void refresh();
    const unlisten = listen("vault-locked", () => {
      setLockReason("O cofre foi travado por inatividade.");
      void refresh();
    });
    return () => void unlisten.then((fn) => fn());
  }, [refresh]);

  // Mantém o cofre aberto enquanto o usuário interage com a janela.
  const unlocked = status?.state === "unlocked";
  useEffect(() => {
    if (!unlocked) return;
    let last = 0;
    const onActivity = () => {
      const now = Date.now();
      if (now - last < TOUCH_EVERY_MS) return;
      last = now;
      void api.touch();
    };
    const events = ["pointerdown", "keydown", "wheel", "pointermove"] as const;
    events.forEach((e) => window.addEventListener(e, onActivity, { passive: true }));
    return () => events.forEach((e) => window.removeEventListener(e, onActivity));
  }, [unlocked]);

  async function lock() {
    await api.lock();
    setLockReason(null);
    await refresh();
  }

  if (status === null) return null;
  if (unlocked) return <Unlocked onLock={lock} />;

  return (
    <Frame>
      {status.state === "none" ? (
        <CreateVault autolockMinutes={Math.round(status.autolockSecs / 60)} onCreated={refresh} />
      ) : (
        <Unlock reason={lockReason} onUnlocked={refresh} />
      )}
    </Frame>
  );
}

function Frame({ children, onLock }: { children: ReactNode; onLock?: () => void }) {
  return (
    <div className="app">
      <header className="topbar">
        <span>Histórico WhatsApp</span>
        {onLock && (
          <button className="ghost" onClick={onLock} title="Travar o cofre">
            🔒 Travar
          </button>
        )}
      </header>
      <main className="content">{children}</main>
      <p className="footnote">🔒 Tudo é processado localmente neste computador. Nada é enviado para servidores.</p>
    </div>
  );
}

function Unlocked({ onLock }: { onLock: () => void }) {
  const [stores, setStores] = useState<StoreInfo[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [importing, setImporting] = useState(false);

  const load = useCallback(async () => {
    try {
      setStores(await api.listStores());
    } catch (e) {
      setError(errorOf(e).message);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  if (error) {
    return (
      <Frame onLock={onLock}>
        <div className="result error">{error}</div>
      </Frame>
    );
  }
  if (stores === null) return null;
  if (stores.length === 0 || importing) {
    return (
      <Frame onLock={onLock}>
        <Home stores={stores} onImported={load} onClose={stores.length ? () => setImporting(false) : undefined} />
      </Frame>
    );
  }
  return <Viewer stores={stores} onLock={onLock} onOpenImport={() => setImporting(true)} />;
}
