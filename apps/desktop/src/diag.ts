// Sends frontend errors to the backend's diagnostics log (the app's stderr).
// Never send message text: only error messages and stacks.
import { invoke } from "@tauri-apps/api/core";

type Level = "error" | "warn" | "info";

let sending = false;

export function uiLog(level: Level, message: string) {
  // Avoids recursion if the invoke itself fails and ends up in console.error.
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

/** Uncaught exceptions, rejected promises and `console.error` go to the log. */
export function installDiagnostics() {
  window.addEventListener("error", (ev) => {
    uiLog("error", `uncaught error: ${describe(ev.error ?? ev.message)} @ ${ev.filename}:${ev.lineno}`);
  });
  window.addEventListener("unhandledrejection", (ev) => {
    uiLog("error", `unhandled promise rejection: ${describe(ev.reason)}`);
  });
  const original = console.error.bind(console);
  console.error = (...args: unknown[]) => {
    original(...args);
    uiLog("error", `console.error: ${args.map(describe).join(" ")}`);
  };
  uiLog("info", `frontend loaded (${navigator.userAgent})`);
}

export { describe };
