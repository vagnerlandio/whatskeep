// Formatação de texto e datas do visualizador.
//
// SEGURANÇA: o texto das mensagens vem de terceiros. Tudo aqui gera nós
// React de texto. Nunca use dangerouslySetInnerHTML, innerHTML nem href
// montado a partir de conteúdo de mensagem.
import { Fragment, ReactNode } from "react";
import { useViewer } from "./context";
import { Icon, IconName } from "./icons";
import type { Chat, MessageKind, Person } from "./types";
import { useLang, type Lang } from "./i18n";
import { VIEWER_MESSAGES } from "./messages";

// ---------------------------------------------------------------- pessoas

/** `5511912345678` → `+55 11 91234-5678`. Outros países: `+<dígitos>`. */
export function formatPhone(phone: string): string {
  const br = /^55(\d{2})(\d{4,5})(\d{4})$/.exec(phone);
  if (br) return `+55 ${br[1]} ${br[2]}-${br[3]}`;
  return `+${phone}`;
}

export function personLabel(p: Person | null, lang: Lang = "pt-BR"): string {
  if (!p) return VIEWER_MESSAGES[lang].unknownPerson;
  if (p.name) return p.name;
  if (p.phone) return formatPhone(p.phone);
  // LID sem telefone conhecido: o WhatsApp mostra "~" antes de quem não está na agenda.
  return `~${p.jid.split("@")[0]}`;
}

export function chatTitle(c: Chat): string {
  if (c.title) return c.title;
  if (c.name) return c.name;
  if (c.phone) return formatPhone(c.phone);
  return c.jid.split("@")[0];
}

const SENDER_COLORS = ["#e542a3", "#1f7aec", "#06cf9c", "#fc9775", "#a62c71", "#02a698", "#d3396d", "#7f66ff", "#bf6e00", "#35a3d6"];

function hash(s: string): number {
  let h = 0;
  for (let i = 0; i < s.length; i++) h = (h * 31 + s.charCodeAt(i)) | 0;
  return Math.abs(h);
}

export function senderColor(jid: string): string {
  return SENDER_COLORS[hash(jid) % SENDER_COLORS.length];
}

/** Matiz estável por JID para o avatar (fundo claro, ícone/letra no mesmo tom). */
export const avatarHue = (jid: string) => [190, 25, 340, 210, 150, 280, 45, 0, 170, 260][hash(jid) % 10];

// ---------------------------------------------------------------- datas

// Formatadores por idioma, criados uma vez cada.
const fmtCache = new Map<string, Intl.DateTimeFormat>();
function fmt(lang: Lang, key: string, opts: Intl.DateTimeFormatOptions): Intl.DateTimeFormat {
  const k = `${lang}|${key}`;
  let f = fmtCache.get(k);
  if (!f) {
    f = new Intl.DateTimeFormat(lang, opts);
    fmtCache.set(k, f);
  }
  return f;
}

export const formatTime = (ms: number, lang: Lang = "pt-BR") =>
  fmt(lang, "time", { hour: "2-digit", minute: "2-digit" }).format(ms);

function startOfDay(ms: number): number {
  const d = new Date(ms);
  d.setHours(0, 0, 0, 0);
  return d.getTime();
}

const DAY = 86_400_000;

/** Chave de dia local, para separar mensagens por data. */
export const dayKey = (ms: number) => startOfDay(ms);

const capitalize = (s: string) => s.charAt(0).toUpperCase() + s.slice(1);

const TODAY: Record<Lang, [string, string]> = { "pt-BR": ["Hoje", "Ontem"], en: ["Today", "Yesterday"] };

/** Separador de dia na conversa: "Hoje", "Ontem", dia da semana ou data por extenso. */
export function formatDay(ms: number, lang: Lang = "pt-BR"): string {
  const diff = Math.round((startOfDay(Date.now()) - startOfDay(ms)) / DAY);
  if (diff === 0) return TODAY[lang][0];
  if (diff === 1) return TODAY[lang][1];
  if (diff > 1 && diff < 7) return capitalize(fmt(lang, "weekday", { weekday: "long" }).format(ms));
  return fmt(lang, "long", { day: "numeric", month: "long", year: "numeric" }).format(ms);
}

/** Hora da lista de conversas: hora (hoje), "Ontem", dia da semana ou data curta. */
export function formatListTime(ms: number, lang: Lang = "pt-BR"): string {
  const diff = Math.round((startOfDay(Date.now()) - startOfDay(ms)) / DAY);
  if (diff === 0) return formatTime(ms, lang);
  if (diff === 1) return TODAY[lang][1];
  if (diff > 1 && diff < 7) return capitalize(fmt(lang, "weekday", { weekday: "long" }).format(ms));
  return formatDate(ms, lang);
}

export const formatDate = (ms: number, lang: Lang = "pt-BR") =>
  fmt(lang, "date", { day: "2-digit", month: "2-digit", year: "numeric" }).format(ms);

/** Data e hora (ex.: "importado em"). */
export const formatDateTime = (ms: number, lang: Lang = "pt-BR") =>
  fmt(lang, "datetime", { dateStyle: "short", timeStyle: "short" }).format(ms);

export function formatDuration(s: number | null): string {
  if (!s || s < 0) return "0:00";
  const m = Math.floor(s / 60);
  const sec = Math.floor(s % 60);
  return m >= 60
    ? `${Math.floor(m / 60)}:${String(m % 60).padStart(2, "0")}:${String(sec).padStart(2, "0")}`
    : `${m}:${String(sec).padStart(2, "0")}`;
}

// ---------------------------------------------------------------- prévias

/** Rótulo de cada tipo de mensagem na prévia, por idioma. */
export const KIND_PREVIEW: Record<Lang, Partial<Record<MessageKind, string>>> = {
  "pt-BR": VIEWER_MESSAGES["pt-BR"].kinds,
  en: VIEWER_MESSAGES.en.kinds,
};

/** Ícone de cada tipo na prévia da lista de conversas, como no WhatsApp. */
export const KIND_ICON: Partial<Record<MessageKind, IconName>> = {
  image: "camera",
  video: "video",
  gif: "gif",
  audio: "headphones",
  voice: "mic",
  sticker: "sticker",
  document: "document",
  contact: "account",
  location: "location",
  live_location: "location",
  deleted: "cancel",
};

export function previewText(kind: MessageKind, text: string | null, lang: Lang = "pt-BR"): string {
  const label = KIND_PREVIEW[lang][kind];
  if (kind === "text") return oneLine(text ?? "");
  if (kind === "deleted" || kind === "system") return label ?? "";
  return text ? oneLine(text) : (label ?? "");
}

/** Prévia com o ícone do tipo (foto, áudio...) antes do texto. */
export function Preview({ kind, text }: { kind: MessageKind; text: string | null }) {
  const { lang } = useLang();
  const icon = KIND_ICON[kind];
  return (
    <>
      {icon && <Icon name={icon} size={16} className="preview-icon" />}
      <span className={kind === "deleted" ? "preview-text muted-italic" : "preview-text"}>{previewText(kind, text, lang)}</span>
    </>
  );
}

const oneLine = (s: string) => s.replace(/\s+/g, " ").trim();

// ---------------------------------------------------------------- marcação do WhatsApp

type Span =
  | string
  | { tag: "b" | "i" | "s" | "code"; children: Span[] }
  | { tag: "pre"; text: string }
  | { tag: "link"; url: string };

// *negrito*, _itálico_, ~tachado~: o marcador precisa estar fora de uma palavra
// e o conteúdo não pode começar nem terminar com espaço (regra do WhatsApp).
const INLINE = /(?<![\p{L}\p{N}])([*_~])(?=\S)([^\n]*?\S)\1(?![\p{L}\p{N}])|`([^`\n]+)`/u;
const TAGS = { "*": "b", _: "i", "~": "s" } as const;

function parseInline(text: string, depth = 0): Span[] {
  if (depth > 4) return [text];
  const out: Span[] = [];
  let rest = text;
  while (rest) {
    const m = INLINE.exec(rest);
    if (!m) {
      out.push(rest);
      break;
    }
    if (m.index > 0) out.push(rest.slice(0, m.index));
    if (m[3] !== undefined) out.push({ tag: "code", children: [m[3]] });
    else out.push({ tag: TAGS[m[1] as keyof typeof TAGS], children: parseInline(m[2], depth + 1) });
    rest = rest.slice(m.index + m[0].length);
  }
  return out;
}

// Só http(s). A pontuação final (ponto, vírgula, parêntese) fica de fora do link.
const URL_RE = /\bhttps?:\/\/[^\s<>"'`]+/gi;

function withLinks(text: string): Span[] {
  const out: Span[] = [];
  let last = 0;
  for (const m of text.matchAll(URL_RE)) {
    let url = m[0];
    const trail = /[.,;:!?)\]}'"]+$/.exec(url);
    if (trail) url = url.slice(0, -trail[0].length);
    const at = m.index!;
    if (at > last) out.push(...parseInline(text.slice(last, at)));
    out.push({ tag: "link", url });
    last = at + url.length;
  }
  if (last < text.length) out.push(...parseInline(text.slice(last)));
  return out;
}

export function parseWaText(text: string): Span[] {
  const out: Span[] = [];
  const parts = text.split(/```([\s\S]+?)```/);
  parts.forEach((part, i) => {
    if (i % 2 === 1) out.push({ tag: "pre", text: part });
    else if (part) out.push(...withLinks(part));
  });
  return out;
}

/** Link de mensagem: nunca vira <a href>. Clicar pede confirmação (veja ChatPane). */
function LinkSpan({ url }: { url: string }) {
  const { openLink } = useViewer();
  return (
    <span
      className="msg-link"
      role="link"
      tabIndex={0}
      title={url}
      onClick={(e) => {
        e.stopPropagation();
        openLink(url);
      }}
      onKeyDown={(e) => e.key === "Enter" && openLink(url)}
    >
      {url}
    </span>
  );
}

function renderSpans(spans: Span[]): ReactNode {
  return spans.map((s, i) => {
    if (typeof s === "string") return <Fragment key={i}>{s}</Fragment>;
    if (s.tag === "pre") return <pre key={i}>{s.text}</pre>;
    if (s.tag === "link") return <LinkSpan key={i} url={s.url} />;
    const Tag = s.tag === "b" ? "strong" : s.tag === "i" ? "em" : s.tag === "s" ? "del" : "code";
    return <Tag key={i}>{renderSpans(s.children)}</Tag>;
  });
}

/** Texto de mensagem com a formatação do WhatsApp, sempre como texto puro. */
export function WaText({ text }: { text: string }) {
  return <>{renderSpans(parseWaText(text))}</>;
}

/** Snippet da busca: trechos entre \u0001 e \u0002 viram <mark>. */
export function Highlight({ text }: { text: string }) {
  const parts = text.split(/\u0001|\u0002/);
  // Os marcadores alternam: índices ímpares são os trechos destacados.
  return (
    <>
      {parts.map((p, i) => (i % 2 === 1 ? <mark key={i}>{p}</mark> : <Fragment key={i}>{p}</Fragment>))}
    </>
  );
}
