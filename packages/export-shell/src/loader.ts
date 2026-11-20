// Loads the bundle's files. Under file:// the browser blocks fetch(), but
// allows <script src>: each data file calls a global function.
import { ShellError } from "./messages";

declare global {
  interface Window {
    __wkManifest?: unknown;
    __wkChunk?: (id: string, b64: string) => void;
  }
}

const pending = new Map<string, (b64: string) => void>();
window.__wkChunk = (id, b64) => {
  pending.get(id)?.(b64);
  pending.delete(id);
};

function loadScript(src: string): Promise<void> {
  return new Promise((resolve, reject) => {
    const s = document.createElement("script");
    s.src = src;
    s.onload = () => {
      s.remove();
      resolve();
    };
    s.onerror = () => {
      s.remove();
      reject(new ShellError("read_failed", src));
    };
    document.head.appendChild(s);
  });
}

export async function loadManifest(): Promise<unknown> {
  await loadScript("data/manifest.js");
  const m = window.__wkManifest;
  delete window.__wkManifest;
  if (!m) throw new ShellError("manifest_missing");
  return m;
}

/** Content (base64) of a chunk. The id comes from the decrypted slot. */
export async function loadChunk(id: string): Promise<string> {
  if (!/^[0-9a-f]{16}$/.test(id)) throw new ShellError("bad_chunk_id");
  const got = new Promise<string>((resolve) => pending.set(id, resolve));
  await loadScript(`data/${id}.js`);
  return got;
}
