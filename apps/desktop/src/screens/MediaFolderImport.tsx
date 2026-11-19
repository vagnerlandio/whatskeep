import { useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { errorOf, mediaApi, MediaProgress, MediaSummary, SourceApp } from "../api";
import { MediaProgressView, MediaSummaryLine } from "./shared";

type State =
  | { kind: "idle"; error?: string }
  | { kind: "running"; progress: MediaProgress | null }
  | { kind: "done"; summary: MediaSummary };

/** Importa as mídias da pasta WhatsApp copiada do celular para o cofre. */
export default function MediaFolderImport({ source }: { source: SourceApp }) {
  const [state, setState] = useState<State>({ kind: "idle" });

  async function run() {
    setState({ kind: "running", progress: null });
    const unlisten = await listen<MediaProgress>("media-progress", (e) =>
      setState((s) => (s.kind === "running" ? { kind: "running", progress: e.payload } : s)),
    );
    try {
      const summary = await mediaApi.importFolder(source);
      setState(summary ? { kind: "done", summary } : { kind: "idle" });
    } catch (e) {
      setState({ kind: "idle", error: errorOf(e).message });
    } finally {
      unlisten();
    }
  }

  if (state.kind === "running") {
    return (
      <div className="media-import">
        <MediaProgressView p={state.progress} onCancel={() => void mediaApi.cancel()} />
      </div>
    );
  }
  return (
    <div className="media-import">
      {state.kind === "done" && <MediaSummaryLine s={state.summary} />}
      {state.kind === "idle" && state.error && <div className="result error">{state.error}</div>}
      <button onClick={run} title="Escolha a pasta WhatsApp (ou WhatsApp Business) copiada do celular">
        {state.kind === "done" ? "Importar mídias de outra pasta…" : "Importar mídias de uma pasta…"}
      </button>
    </div>
  );
}
