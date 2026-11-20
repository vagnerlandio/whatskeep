import { ReactNode, useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { detectLang, Icon, Lang, LangProvider, LanguageSelect } from "@wk/viewer";
import { api, settingsApi, StoreInfo, VaultStatus } from "./api";
import { useAppT } from "./i18n/app";
import { useErrorText } from "./i18n/errors";
import Home, { DownloadOnly } from "./screens/Home";
import Unlock from "./screens/Unlock";
import AppViewer from "./screens/AppViewer";
import { JobProvider } from "./jobs";
import { VaultGateProvider, VaultState } from "./vault";
import "@wk/viewer/base.css";
import "@wk/viewer/viewer.css";
import "./App.css";

/** Minimum interval between activity pings to the backend (autolock). */
const TOUCH_EVERY_MS = 15_000;

/** Loads the saved language (or the system one) before drawing any screen. */
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
  // Locked due to inactivity: the text comes from the dictionary when displayed.
  const [lockedIdle, setLockedIdle] = useState(false);
  // Locked vault, but the user only wants to save a backup copy to a folder.
  const [downloadOnly, setDownloadOnly] = useState(false);
  // Bumped when a job changes what is in the vault.
  const [storesVersion, setStoresVersion] = useState(0);

  const refresh = useCallback(async () => setStatus(await api.status()), []);

  useEffect(() => {
    void refresh();
    const unlisten = listen("vault-locked", () => {
      setLockedIdle(true);
      void refresh();
    });
    return () => void unlisten.then((fn) => fn());
  }, [refresh]);

  // Keeps the vault open while the user interacts with the window.
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

  const storesChanged = useCallback(() => setStoresVersion((v) => v + 1), []);

  if (status === null) return null;
  const autolockMinutes = Math.round(status.autolockSecs / 60);

  return (
    // Imports and downloads outlive the screen that started them.
    <JobProvider onStoresChanged={storesChanged}>
      {status.state === "locked" ? (
        <Frame>
          {downloadOnly ? (
            <DownloadOnly onBack={() => setDownloadOnly(false)} />
          ) : (
            <Unlock
              reason={lockedIdle ? t.lockedIdle : null}
              onUnlocked={() => {
                setDownloadOnly(false);
                void refresh();
              }}
              onDownloadOnly={() => setDownloadOnly(true)}
              onDeleted={() => {
                setLockedIdle(false);
                void refresh();
              }}
            />
          )}
        </Frame>
      ) : (
        <VaultGateProvider vault={status.state} autolockMinutes={autolockMinutes} onCreated={refresh}>
          <Main vault={status.state} storesVersion={storesVersion} onLock={lock} onVaultDeleted={refresh} />
        </VaultGateProvider>
      )}
    </JobProvider>
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

/**
 * Without a vault, or with it open: the import screen while there is no
 * history (or the user opened it), otherwise the viewer. Creating the vault
 * on demand keeps this mounted, so the import in progress is not lost.
 */
type MainProps = { vault: VaultState; storesVersion: number; onLock: () => void; onVaultDeleted: () => void };

function Main({ vault, storesVersion, onLock, onVaultDeleted }: MainProps) {
  const errorText = useErrorText();
  const [stores, setStores] = useState<StoreInfo[] | null>(null);
  // Stores the raw error: the text follows language changes.
  const [error, setError] = useState<{ e: unknown } | null>(null);
  const [importing, setImporting] = useState(false);

  const load = useCallback(async () => {
    if (vault !== "unlocked") return setStores([]);
    try {
      setStores(await api.listStores());
    } catch (e) {
      setError({ e });
    }
  }, [vault]);

  useEffect(() => {
    void load();
  }, [load, storesVersion]);

  const lock = vault === "unlocked" ? onLock : undefined;
  if (error) {
    return (
      <Frame onLock={lock}>
        <div className="result error">{errorText(error.e)}</div>
      </Frame>
    );
  }
  if (stores === null) return null;
  if (stores.length === 0 || importing) {
    return (
      <Frame onLock={lock}>
        <Home
          stores={stores}
          onClose={stores.length ? () => setImporting(false) : undefined}
          onStoresChanged={load}
          onVaultDeleted={() => {
            setImporting(false);
            onVaultDeleted();
          }}
        />
      </Frame>
    );
  }
  return <AppViewer stores={stores} onLock={onLock} onOpenImport={() => setImporting(true)} />;
}
