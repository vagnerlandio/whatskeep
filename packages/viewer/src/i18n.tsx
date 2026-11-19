// Idiomas da interface (português e inglês), compartilhados pelo app e pelo
// export HTML.
//
// Cada pacote tem os seus dicionários, tipados: o dicionário em inglês é
// declarado com o tipo do português, então o TypeScript acusa qualquer texto
// que falte em um dos idiomas. Textos com valores são funções.
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

/** Idioma do sistema/navegador: português se começar com "pt", senão inglês. */
export function detectLang(): Lang {
  const langs = typeof navigator === "undefined" ? [] : (navigator.languages ?? [navigator.language]);
  return langs.some((l) => l?.toLowerCase().startsWith("pt")) ? "pt-BR" : "en";
}

type LangState = { lang: Lang; setLang: (l: Lang) => void };

const LangContext = createContext<LangState>({ lang: "pt-BR", setLang: () => {} });

/** Fornece o idioma atual. Quem guarda a escolha é quem usa (o app salva no
 * backend; o export HTML só lembra durante a sessão). */
export function LangProvider({ lang, setLang, children }: LangState & { children: ReactNode }) {
  useEffect(() => {
    document.documentElement.lang = lang;
  }, [lang]);
  const value = useMemo(() => ({ lang, setLang }), [lang, setLang]);
  return <LangContext.Provider value={value}>{children}</LangContext.Provider>;
}

export const useLang = () => useContext(LangContext);

/** Cria o hook de traduções de um dicionário. Componentes que o usam
 * re-renderizam ao trocar o idioma (inclusive os `memo`). */
export function makeT<M>(dict: Record<Lang, M>): () => M {
  return function useT() {
    return dict[useLang().lang];
  };
}

const LANG_DETAIL: Record<Lang, { short: string; region: string }> = {
  // Linha secundária: o nome na outra língua, para quem não lê a primeira.
  "pt-BR": { short: "PT", region: "Portuguese (Brazil)" },
  en: { short: "EN", region: "Inglês" },
};

/** Seletor de idioma: botão com o idioma atual e um menu com as opções.
 *
 * * `pill` (padrão): botão com ícone, nome e seta; herda a cor do texto ao
 *   redor, então serve tanto na barra verde quanto num cartão branco.
 * * `rail`: só o ícone (com a sigla), para a barra de navegação; o menu abre
 *   ao lado. */
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
