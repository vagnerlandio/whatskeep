// Primeiro import: congela os protótipos antes de qualquer outro código do app.
import "./freeze";
import React, { Component, ErrorInfo, ReactNode } from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { describe, installDiagnostics, uiLog } from "./diag";
import { detectLang } from "@wa/viewer";

installDiagnostics();

// A tela de erro fica fora do LangProvider: usa o idioma do sistema.
const CRASH_TEXT = {
  "pt-BR": { title: "Algo deu errado", reload: "Recarregar" },
  en: { title: "Something went wrong", reload: "Reload" },
};

/** Falha de renderização: registra no log e mostra uma tela de erro em vez
 * de deixar a janela em branco. */
class ErrorBoundary extends Component<{ children: ReactNode }, { error: Error | null }> {
  state = { error: null as Error | null };

  static getDerivedStateFromError(error: Error) {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    uiLog("error", `falha de renderização: ${describe(error)}\ncomponentes:${(info.componentStack ?? "").split("\n").slice(0, 8).join("\n")}`);
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
