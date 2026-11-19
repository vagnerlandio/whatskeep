import { FormEvent, ReactNode, useEffect, useRef, useState } from "react";
import { detectLang, Icon, Lang, LangProvider, LanguageSelect, RailButton, useLang, Viewer } from "@wa/viewer";
import { BundleSource } from "./bundle";
import { loadManifest } from "./loader";
import { errorText, useShellT } from "./messages";

/** With no interaction for this long, the page forgets everything (reloads). */
const AUTOLOCK_MS = 15 * 60 * 1000;

export default function App() {
  // The language choice only lasts for this session: the page stores nothing.
  const [lang, setLang] = useState<Lang>(detectLang);
  return (
    <LangProvider lang={lang} setLang={setLang}>
      <Shell />
    </LangProvider>
  );
}

function Shell() {
  const t = useShellT();
  const { lang } = useLang();
  const [source, setSource] = useState<BundleSource | null>(null);
  const [fatal, setFatal] = useState(false);
  const [unlocked, setUnlocked] = useState(false);
  const [version, setVersion] = useState(0);
  const [askMore, setAskMore] = useState(false);
  const folderInput = useRef<HTMLInputElement>(null);

  useEffect(() => {
    loadManifest()
      .then((m) => setSource(new BundleSource(m as ConstructorParameters<typeof BundleSource>[0])))
      .catch(() => setFatal(true));
  }, []);

  // The media hints come from the BundleSource; it follows the screen's language.
  if (source) source.lang = lang;

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
      alert(t.noMediaFound);
      return;
    }
    setVersion((v) => v + 1); // remounts the viewer to load the media
  }

  if (fatal) return <Centered><div className="result error">{t.cantRead}</div></Centered>;
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
      <RailButton icon="key" title={t.unlockMore} onClick={() => setAskMore(true)} />
      {source.mediaMode === "encrypted" && (
        <RailButton
          icon="folder"
          title={t.pickFolderTitle}
          onClick={() => folderInput.current?.click()}
        />
      )}
      <RailButton icon="lock" title={t.lock} onClick={lock} />
      <input
        ref={folderInput}
        type="file"
        hidden
        // Selects the whole folder: the browser only reads files the user picks.
        {...{ webkitdirectory: "", directory: "" }}
        onChange={(e) => onFolder(e.target.files)}
      />
    </>
  );
  const banner = needsFolder ? (
    <div className="folder-banner">
      <span>{t.folderBanner}</span>
      <button onClick={() => folderInput.current?.click()}>{t.pickFolder}</button>
    </div>
  ) : null;

  return (
    <Viewer
      key={version}
      source={source}
      title={t.viewerTitle}
      railBottom={railBottom}
      banner={banner}
      emptyTitle={t.title}
      emptyNote={t.emptyNote(source.unlockedCount)}
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
  const t = useShellT();
  const { lang } = useLang();
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);
  // Stores the error itself (not the text) so it switches language along with the screen.
  const [error, setError] = useState<"wrong" | "noNew" | { err: unknown } | null>(null);

  async function submit(e: FormEvent) {
    e.preventDefault();
    if (!password || busy) return;
    setBusy(true);
    setError(null);
    // Lets "Opening…" show up before Argon2id hogs the page.
    await new Promise((r) => setTimeout(r, 30));
    try {
      const added = await source.unlock(password);
      setPassword("");
      if (added === 0) {
        setError(more ? "noNew" : "wrong");
        setBusy(false);
        return;
      }
      onDone();
    } catch (err) {
      setError({ err });
      setBusy(false);
    }
  }

  return (
    <form className="card narrow" onSubmit={submit}>
      <LanguageSelect className="unlock-lang" />
      <div className="lock-icon">
        <Icon name="lockKeyhole" size={40} strokeWidth={1.5} />
      </div>
      <h1>{more ? t.moreTitle : t.title}</h1>
      {!more && <p className="tagline">{t.tagline}</p>}
      <p className="muted">
        {more ? t.moreIntro : t.intro}
      </p>
      <label>
        {t.password}
        <input
          type="password"
          autoFocus
          autoComplete="off"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          disabled={busy}
        />
      </label>
      {error && (
        <div className="result error">
          {error === "wrong" ? t.wrongPassword : error === "noNew" ? t.noNewChats : errorText(error.err, lang)}
        </div>
      )}
      <div className="row-buttons">
        {onCancel && (
          <button type="button" onClick={onCancel} disabled={busy}>
            {t.back}
          </button>
        )}
        <button className="primary" type="submit" disabled={!password || busy}>
          {busy ? t.opening : t.open}
        </button>
      </div>
    </form>
  );
}
