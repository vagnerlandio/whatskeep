// Formatação de texto e datas do visualizador.
//
// SEGURANÇA: o texto das mensagens vem de terceiros. Tudo aqui gera nós
// React de texto. Nunca use dangerouslySetInnerHTML, innerHTML nem href
// montado a partir de conteúdo de mensagem.
import { Fragment, ReactNode } from "react";
import type { Chat, MessageKind, Person } from "../api";

// ---------------------------------------------------------------- pessoas

/** `5511912345678` → `+55 11 91234-5678`. Outros países: `+<dígitos>`. */
export function formatPhone(phone: string): string {
  const br = /^55(\d{2})(\d{4,5})(\d{4})$/.exec(phone);
  if (br) return `+55 ${br[1]} ${br[2]}-${br[3]}`;
  return `+${phone}`;
}

export function personLabel(p: Person | null): string {
  if (!p) return "Desconhecido";
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

export function senderColor(jid: string): string {
  let h = 0;
  for (let i = 0; i < jid.length; i++) h = (h * 31 + jid.charCodeAt(i)) | 0;
  return SENDER_COLORS[Math.abs(h) % SENDER_COLORS.length];
}

// ---------------------------------------------------------------- datas

const timeFmt = new Intl.DateTimeFormat("pt-BR", { hour: "2-digit", minute: "2-digit" });
const dateFmt = new Intl.DateTimeFormat("pt-BR", { day: "2-digit", month: "2-digit", year: "numeric" });
const weekdayFmt = new Intl.DateTimeFormat("pt-BR", { weekday: "long" });
const longDateFmt = new Intl.DateTimeFormat("pt-BR", { day: "numeric", month: "long", year: "numeric" });

export const formatTime = (ms: number) => timeFmt.format(ms);

function startOfDay(ms: number): number {
  const d = new Date(ms);
  d.setHours(0, 0, 0, 0);
  return d.getTime();
}

const DAY = 86_400_000;

/** Chave de dia local, para separar mensagens por data. */
export const dayKey = (ms: number) => startOfDay(ms);

const capitalize = (s: string) => s.charAt(0).toUpperCase() + s.slice(1);

/** Separador de dia na conversa: "Hoje", "Ontem", dia da semana ou data por extenso. */
export function formatDay(ms: number): string {
  const diff = Math.round((startOfDay(Date.now()) - startOfDay(ms)) / DAY);
  if (diff === 0) return "Hoje";
  if (diff === 1) return "Ontem";
  if (diff > 1 && diff < 7) return capitalize(weekdayFmt.format(ms));
  return longDateFmt.format(ms);
}

/** Hora da lista de conversas: hora (hoje), "Ontem", dia da semana ou dd/mm/aaaa. */
export function formatListTime(ms: number): string {
  const diff = Math.round((startOfDay(Date.now()) - startOfDay(ms)) / DAY);
  if (diff === 0) return formatTime(ms);
  if (diff === 1) return "Ontem";
  if (diff > 1 && diff < 7) return capitalize(weekdayFmt.format(ms));
  return dateFmt.format(ms);
}

export const formatDate = (ms: number) => dateFmt.format(ms);

export function formatDuration(s: number | null): string {
  if (!s || s < 0) return "0:00";
  const m = Math.floor(s / 60);
  const sec = Math.floor(s % 60);
  return m >= 60
    ? `${Math.floor(m / 60)}:${String(m % 60).padStart(2, "0")}:${String(sec).padStart(2, "0")}`
    : `${m}:${String(sec).padStart(2, "0")}`;
}

// ---------------------------------------------------------------- prévias

export const KIND_PREVIEW: Partial<Record<MessageKind, string>> = {
  image: "📷 Foto",
  video: "🎥 Vídeo",
  gif: "🎞️ GIF",
  audio: "🎵 Áudio",
  voice: "🎤 Mensagem de voz",
  sticker: "💟 Figurinha",
  document: "📄 Documento",
  contact: "👤 Contato",
  location: "📍 Localização",
  live_location: "📍 Localização em tempo real",
  deleted: "🚫 Mensagem apagada",
  system: "Mensagem do sistema",
  unknown: "Mensagem não suportada",
};

export function previewText(kind: MessageKind, text: string | null): string {
  const label = KIND_PREVIEW[kind];
  if (kind === "text") return oneLine(text ?? "");
  if (kind === "deleted" || kind === "system") return label ?? "";
  if (kind === "contact" && text) return `👤 ${oneLine(text)}`;
  return text ? `${label ? label.split(" ")[0] + " " : ""}${oneLine(text)}` : (label ?? "");
}

const oneLine = (s: string) => s.replace(/\s+/g, " ").trim();

// ---------------------------------------------------------------- marcação do WhatsApp

type Span = string | { tag: "b" | "i" | "s" | "code"; children: Span[] } | { tag: "pre"; text: string };

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

export function parseWaText(text: string): Span[] {
  const out: Span[] = [];
  const parts = text.split(/```([\s\S]+?)```/);
  parts.forEach((part, i) => {
    if (i % 2 === 1) out.push({ tag: "pre", text: part });
    else if (part) out.push(...parseInline(part));
  });
  return out;
}

function renderSpans(spans: Span[]): ReactNode {
  return spans.map((s, i) => {
    if (typeof s === "string") return <Fragment key={i}>{s}</Fragment>;
    if (s.tag === "pre") return <pre key={i}>{s.text}</pre>;
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
