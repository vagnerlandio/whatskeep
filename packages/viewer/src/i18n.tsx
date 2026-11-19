// UI languages (Portuguese and English), shared by the app and the
// HTML export.
//
// Each package has its own typed dictionaries: the English dictionary is
// declared with the Portuguese one's type, so TypeScript flags any text
// missing from either language. Texts with values are functions.
//
//   const pt = { title: "Conversas", count: (n: number) => `${n} conversas` };
//   const en: typeof pt = { title: "Chats", count: (n) => `${n} chats` };
//   export const useMyT = makeT({ "pt-BR": pt, en });
//   ...
//   const t = useMyT();  t.title  t.count(3)
import { createContext, ReactNode, useContext, useEffect, useMemo, useRef, useState } from "react";
import { Icon } from "./icons";

export type Lang = "pt-BR" | "en";

export const LANGS: { code: Lang; label: string }[] = [
  { code: "pt-BR", label: "Português" },
  { code: "en", label: "English" },
];

export function isLang(v: unknown): v is Lang {
  return v === "pt-BR" || v === "en";
}

/** System/browser language: Portuguese if it starts with "pt", otherwise English. */
export function detectLang(): Lang {
  const langs = typeof navigator === "undefined" ? [] : (navigator.languages ?? [navigator.language]);
  return langs.some((l) => l?.toLowerCase().startsWith("pt")) ? "pt-BR" : "en";
}

type LangState = { lang: Lang; setLang: (l: Lang) => void };

const LangContext = createContext<LangState>({ lang: "pt-BR", setLang: () => {} });

/** Provides the current language. Persisting the choice is up to the caller (the app
 * saves it in the backend; the HTML export only remembers it for the session). */
export function LangProvider({ lang, setLang, children }: LangState & { children: ReactNode }) {
  useEffect(() => {
    document.documentElement.lang = lang;
  }, [lang]);
  const value = useMemo(() => ({ lang, setLang }), [lang, setLang]);
  return <LangContext.Provider value={value}>{children}</LangContext.Provider>;
}

export const useLang = () => useContext(LangContext);

/** Creates the translation hook for a dictionary. Components that use it
 * re-render when the language changes (including `memo` ones). */
export function makeT<M>(dict: Record<Lang, M>): () => M {
  return function useT() {
    return dict[useLang().lang];
  };
}

const LANG_DETAIL: Record<Lang, { short: string; region: string }> = {
  // Secondary line: the name in the other language, for those who can't read the first.
  "pt-BR": { short: "PT", region: "Portuguese (Brazil)" },
  en: { short: "EN", region: "Inglês" },
};

/** Language selector: a button with the current language and a menu with the options.
 *
 * * `pill` (default): button with icon, name and chevron; inherits the surrounding
 *   text color, so it works both on the green bar and on a white card.
 * * `rail`: icon only (with the code), for the navigation rail; the menu opens
 *   to the side. */
export function LanguageSelect({ variant = "pill", className }: { variant?: "pill" | "rail"; className?: string }) {
  const { lang, setLang } = useLang();
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLDivElement>(null);
  const current = LANGS.find((l) => l.code === lang) ?? LANGS[0];

  useEffect(() => {
    if (!open) return;
    const onDown = (e: PointerEvent) => {
      if (!root.current?.contains(e.target as Node)) setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && setOpen(false);
    document.addEventListener("pointerdown", onDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("pointerdown", onDown);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  function choose(l: Lang) {
    setLang(l);
    setOpen(false);
  }

  return (
    <div ref={root} className={["lang-menu", `lang-menu-${variant}`, open ? "open" : "", className ?? ""].join(" ")}>
      <button
        type="button"
        className={variant === "rail" ? "rail-btn lang-trigger" : "lang-trigger"}
        aria-haspopup="menu"
        aria-expanded={open}
        title="Idioma · Language"
        onClick={() => setOpen((o) => !o)}
      >
        <Icon name="languages" size={variant === "rail" ? 22 : 16} />
        {variant === "rail" ? (
          <span className="rail-badge">{LANG_DETAIL[lang].short}</span>
        ) : (
          <>
            <span className="lang-current">{current.label}</span>
            <Icon name="chevronDown" size={14} className="lang-chevron" />
          </>
        )}
      </button>
      {open && (
        <div className="lang-popover" role="menu">
          <div className="lang-caption">Idioma · Language</div>
          {LANGS.map((l) => {
            const selected = l.code === lang;
            return (
              <button
                key={l.code}
                type="button"
                role="menuitemradio"
                aria-checked={selected}
                className={selected ? "lang-option selected" : "lang-option"}
                onClick={() => choose(l.code)}
              >
                <span className="lang-code">{LANG_DETAIL[l.code].short}</span>
                <span className="lang-names">
                  <span>{l.label}</span>
                  <small>{LANG_DETAIL[l.code].region}</small>
                </span>
                {selected && <Icon name="check" size={18} className="lang-check" />}
              </button>
            );
          })}
        </div>
      )}
    </div>
  );
}
