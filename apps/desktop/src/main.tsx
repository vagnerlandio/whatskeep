// First import: freezes the prototypes before any other app code.
import "./freeze";
import React, { Component, ErrorInfo, ReactNode } from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { describe, installDiagnostics, uiLog } from "./diag";
import { detectLang } from "@wa/viewer";

installDiagnostics();

// The error screen sits outside the LangProvider: it uses the system language.
const CRASH_TEXT = {
  "pt-BR": { title: "Algo deu errado", reload: "Recarregar" },
  en: { title: "Something went wrong", reload: "Reload" },
};

/** Render failure: logs it and shows an error screen instead of
 * leaving the window blank. */
class ErrorBoundary extends Component<{ children: ReactNode }, { error: Error | null }> {
  state = { error: null as Error | null };

  static getDerivedStateFromError(error: Error) {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    uiLog("error", `render failure: ${describe(error)}\ncomponents:${(info.componentStack ?? "").split("\n").slice(0, 8).join("\n")}`);
  }

  render() {
    if (!this.state.error) return this.props.children;
    const t = CRASH_TEXT[detectLang()];
    return (
      <div className="app">
        <main className="content">
          <div className="card narrow">
            <h2>{t.title}</h2>
            <p className="muted">{this.state.error.message}</p>
            <button className="primary" onClick={() => location.reload()}>
              {t.reload}
            </button>
          </div>
        </main>
      </div>
    );
  }
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <ErrorBoundary>
      <App />
    </ErrorBoundary>
  </React.StrictMode>,
);
