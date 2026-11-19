import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, VaultStatus } from "./api";
import CreateVault from "./screens/CreateVault";
import Home from "./screens/Home";
import Unlock from "./screens/Unlock";
import "./App.css";

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

  return (
    <div className="app">
      <header className="topbar">
        <span>Histórico WhatsApp</span>
        {unlocked && (
          <button className="ghost" onClick={lock} title="Travar o cofre">
            🔒 Travar
          </button>
        )}
      </header>
      <main className="content">
        {status === null ? null : status.state === "none" ? (
          <CreateVault autolockMinutes={Math.round(status.autolockSecs / 60)} onCreated={refresh} />
        ) : status.state === "locked" ? (
          <Unlock reason={lockReason} onUnlocked={refresh} />
        ) : (
          <Home />
        )}
      </main>
      <p className="footnote">🔒 Tudo é processado localmente neste computador. Nada é enviado para servidores.</p>
    </div>
  );
}
