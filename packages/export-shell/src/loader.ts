// Carrega os arquivos do pacote. Em file:// o navegador bloqueia fetch(), mas
// permite <script src>: cada arquivo de dados chama uma função global.

declare global {
  interface Window {
    __waManifest?: unknown;
    __waChunk?: (id: string, b64: string) => void;
  }
}

const pending = new Map<string, (b64: string) => void>();
window.__waChunk = (id, b64) => {
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
      reject(new Error(`não foi possível ler ${src}`));
    };
    document.head.appendChild(s);
  });
}

export async function loadManifest(): Promise<unknown> {
  await loadScript("data/manifest.js");
  const m = window.__waManifest;
  delete window.__waManifest;
  if (!m) throw new Error("manifest ausente");
  return m;
}

/** Conteúdo (base64) de um pedaço. O id vem do slot decifrado. */
export async function loadChunk(id: string): Promise<string> {
  if (!/^[0-9a-f]{16}$/.test(id)) throw new Error("id de pedaço inválido");
  const got = new Promise<string>((resolve) => pending.set(id, resolve));
  await loadScript(`data/${id}.js`);
  return got;
}
