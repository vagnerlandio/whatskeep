import { ReactNode, useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { detectLang, Icon, Lang, LangProvider, LanguageSelect } from "@wa/viewer";
import { api, settingsApi, StoreInfo, VaultStatus } from "./api";
import { useAppT } from "./i18n/app";
import { useErrorText } from "./i18n/errors";
import CreateVault from "./screens/CreateVault";
import Home from "./screens/Home";
import Unlock from "./screens/Unlock";
import AppViewer from "./screens/AppViewer";
import "@wa/viewer/base.css";
import "@wa/viewer/viewer.css";
import "./App.css";

/** Intervalo mínimo entre avisos de atividade ao backend (autolock). */
const TOUCH_EVERY_MS = 15_000;

/** Carrega o idioma salvo (ou o do sistema) antes de desenhar qualquer tela. */
export default function App() {
  const [lang, setLangState] = useState<Lang | null>(null);
  useEffect(() => {
    settingsApi
      .getLanguage()
      .then((l) => setLangState(l ?? detectLang()))
      .catch(() => setLangState(detectLang()));
  }, []);
  const setLang = useCallback((l: Lang) => {
    setLangState(l);
    void settingsApi.setLanguage(l).catch(() => {});
  }, []);
  if (!lang) return null;
  return (
    <LangProvider lang={lang} setLang={setLang}>
      <AppScreens />
    </LangProvider>
  );
}

function AppScreens() {
  const t = useAppT();
  const [status, setStatus] = useState<VaultStatus | null>(null);
  // Travado por inatividade: o texto sai do dicionário na hora de mostrar.
  const [lockedIdle, setLockedIdle] = useState(false);

  const refresh = useCallback(async () => setStatus(await api.status()), []);

  useEffect(() => {
    void refresh();
    const unlisten = listen("vault-locked", () => {
      setLockedIdle(true);
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
    setLockedIdle(false);
    await refresh();
  }

  if (status === null) return null;
  if (unlocked) return <Unlocked onLock={lock} />;

  return (
    <Frame>
      {status.state === "none" ? (
        <CreateVault autolockMinutes={Math.round(status.autolockSecs / 60)} onCreated={refresh} />
      ) : (
        <Unlock reason={lockedIdle ? t.lockedIdle : null} onUnlocked={refresh} />
      )}
    </Frame>
  );
}

function Frame({ children, onLock }: { children: ReactNode; onLock?: () => void }) {
  const t = useAppT();
  return (
    <div className="app">
      <header className="topbar">
        <span className="brand">
          <strong>{t.appName}</strong>
          <small>{t.tagline}</small>
        </span>
        <span className="topbar-actions">
          <LanguageSelect />
          {onLock && (
          <button className="ghost" onClick={onLock} title={t.lockTitle}>
            <Icon name="lock" size={16} /> {t.lock}
          </button>
          )}
        </span>
      </header>
      <main className="content">{children}</main>
      <p className="footnote">
        <Icon name="lock" size={14} /> {t.footnote}
      </p>
    </div>
  );
}

function Unlocked({ onLock }: { onLock: () => void }) {
  const errorText = useErrorText();
  const [stores, setStores] = useState<StoreInfo[] | null>(null);
  // Guarda o erro bruto: o texto acompanha a troca de idioma.
  const [error, setError] = useState<{ e: unknown } | null>(null);
  const [importing, setImporting] = useState(false);

  const load = useCallback(async () => {
    try {
      setStores(await api.listStores());
    } catch (e) {
      setError({ e });
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  if (error) {
    return (
      <Frame onLock={onLock}>
        <div className="result error">{errorText(error.e)}</div>
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
  return <AppViewer stores={stores} onLock={onLock} onOpenImport={() => setImporting(true)} />;
}
