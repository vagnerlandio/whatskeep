// Envia erros do frontend para o log de diagnóstico do backend (stderr do app).
// Nunca mande texto de mensagens: só mensagens de erro e pilhas.
import { invoke } from "@tauri-apps/api/core";

type Level = "error" | "warn" | "info";

let sending = false;

export function uiLog(level: Level, message: string) {
  // Evita recursão se o próprio invoke falhar e cair no console.error.
  if (sending) return;
  sending = true;
  invoke("ui_log", { level, message: message.slice(0, 1000) })
    .catch(() => {})
    .finally(() => {
      sending = false;
    });
}

function describe(e: unknown): string {
  if (e instanceof Error) return `${e.name}: ${e.message}\n${(e.stack ?? "").split("\n").slice(0, 6).join("\n")}`;
  if (e && typeof e === "object" && "code" in e && "message" in e) return `[${String(e.code)}] ${String(e.message)}`;
  return String(e);
}

/** Exceções não tratadas, promessas rejeitadas e `console.error` vão para o log. */
export function installDiagnostics() {
  window.addEventListener("error", (ev) => {
    uiLog("error", `erro não tratado: ${describe(ev.error ?? ev.message)} @ ${ev.filename}:${ev.lineno}`);
  });
  window.addEventListener("unhandledrejection", (ev) => {
    uiLog("error", `promessa rejeitada sem tratamento: ${describe(ev.reason)}`);
  });
  const original = console.error.bind(console);
  console.error = (...args: unknown[]) => {
    original(...args);
    uiLog("error", `console.error: ${args.map(describe).join(" ")}`);
  };
  uiLog("info", `frontend carregado (${navigator.userAgent})`);
}

export { describe };
