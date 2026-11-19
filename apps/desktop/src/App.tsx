import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import "./App.css";

type BackupInfo = { version: "crypt14" | "crypt15"; sizeBytes: number };

type State =
  | { kind: "idle" }
  | { kind: "ok"; name: string; info: BackupInfo }
  | { kind: "error"; name: string; message: string };

function formatSize(bytes: number): string {
  const units = ["B", "KB", "MB", "GB"];
  let v = bytes;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v.toFixed(i === 0 ? 0 : 1)} ${units[i]}`;
}

const KEY_HINT: Record<BackupInfo["version"], string> = {
  crypt15:
    "Backup com criptografia de ponta a ponta. Você vai precisar da chave de 64 dígitos (ou do arquivo encrypted_backup.key). Backups protegidos só por senha não podem ser abertos fora do WhatsApp.",
  crypt14: "Backup sem criptografia de ponta a ponta. Você vai precisar do arquivo key do celular, que só pode ser extraído com root.",
};

export default function App() {
  const [state, setState] = useState<State>({ kind: "idle" });

  useEffect(() => {
    const unlisten = getCurrentWebview().onDragDropEvent(async (event) => {
      if (event.payload.type !== "drop") return;
      const path = event.payload.paths[0];
      if (!path) return;
      const name = path.split(/[\\/]/).pop() ?? path;
      try {
        const info = await invoke<BackupInfo>("inspect_backup", { path });
        setState({ kind: "ok", name, info });
      } catch (e) {
        setState({ kind: "error", name, message: String(e) });
      }
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  return (
    <div className="app">
      <header className="topbar">Histórico WhatsApp</header>
      <main className="welcome">
        <div className="dropzone">
          <p className="title">Arraste um arquivo de backup para cá</p>
          <p className="subtitle">msgstore.db.crypt15 ou msgstore.db.crypt14</p>

          {state.kind === "ok" && (
            <div className="result">
              <strong>{state.name}</strong>
              <span>
                {state.info.version} · {formatSize(state.info.sizeBytes)}
              </span>
              <p>{KEY_HINT[state.info.version]}</p>
            </div>
          )}
          {state.kind === "error" && (
            <div className="result error">
              <strong>{state.name}</strong>
              <p>{state.message}</p>
            </div>
          )}
        </div>
        <p className="footnote">🔒 Tudo é processado localmente neste computador. Nada é enviado para servidores.</p>
      </main>
    </div>
  );
}
