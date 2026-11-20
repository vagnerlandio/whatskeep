// HTML export texts (Portuguese and English). The English one has the
// Portuguese one's type, so TypeScript flags any missing text.
import { Lang, makeT } from "@wk/viewer";

const pt = {
  // Password screen
  title: "WhatsKeep",
  tagline: "Suas conversas do WhatsApp, seguras e acessíveis.",
  moreTitle: "Outra senha",
  intro: "Este histórico é cifrado. Digite a senha definida na exportação.",
  moreIntro: "Cada senha libera as conversas que foram exportadas com ela.",
  password: "Senha",
  back: "Voltar",
  open: "Abrir",
  opening: "Abrindo…",
  wrongPassword: "Senha incorreta.",
  noNewChats: "Essa senha não liberou nenhuma conversa nova.",

  // Viewer
  viewerTitle: "Histórico",
  unlockMore: "Liberar mais conversas com outra senha",
  pickFolderTitle: "Selecionar a pasta do histórico (fotos e vídeos)",
  lock: "Travar (esquece tudo o que foi aberto)",
  folderBanner: "Para ver fotos e vídeos, selecione a pasta deste histórico.",
  pickFolder: "Selecionar pasta",
  noMediaFound: "Nenhuma mídia encontrada. Selecione a pasta que contém o index.html deste histórico.",
  emptyNote: (n: number) =>
    `${n} conversas liberadas. Nada sai deste computador: a página não acessa a internet.`,

  // Media hints (shown by the viewer)
  hintPickFolder: "Para ver fotos e vídeos, selecione a pasta deste histórico (ícone de pasta na barra lateral).",
  hintTooLarge: "Arquivo grande demais para abrir no navegador.",

  // Errors
  cantRead:
    "Não foi possível ler os dados do histórico. Abra o index.html de dentro da pasta exportada (com a pasta data ao lado).",
  errors: {
    unsupported_format: () => "versão do pacote não suportada",
    unknown_kdf: () => "KDF desconhecido",
    read_failed: (file: string) => `não foi possível ler ${file}`,
    manifest_missing: () => "manifest ausente",
    bad_chunk_id: () => "id de pedaço inválido",
    chat_locked: () => "conversa não liberada",
    media_truncated: () => "pacote de mídia truncado",
  } satisfies Record<string, (detail: string) => string>,
};

export type ShellMessages = typeof pt;
export type ErrorCode = keyof ShellMessages["errors"];

const en: ShellMessages = {
  title: "WhatsKeep",
  tagline: "Your WhatsApp conversations, safe and accessible.",
  moreTitle: "Another password",
  intro: "This history is encrypted. Enter the password set when it was exported.",
  moreIntro: "Each password unlocks the chats that were exported with it.",
  password: "Password",
  back: "Back",
  open: "Open",
  opening: "Opening…",
  wrongPassword: "Wrong password.",
  noNewChats: "This password didn't unlock any new chats.",

  viewerTitle: "History",
  unlockMore: "Unlock more chats with another password",
  pickFolderTitle: "Select the history folder (photos and videos)",
  lock: "Lock (forgets everything that was opened)",
  folderBanner: "To see photos and videos, select this history's folder.",
  pickFolder: "Select folder",
  noMediaFound: "No media found. Select the folder that contains this history's index.html.",
  emptyNote: (n) =>
    `${n} ${n === 1 ? "chat" : "chats"} unlocked. Nothing leaves this computer: the page doesn't access the internet.`,

  hintPickFolder: "To see photos and videos, select this history's folder (folder icon in the side bar).",
  hintTooLarge: "File too large to open in the browser.",

  cantRead:
    "Couldn't read the history data. Open the index.html from inside the exported folder (with the data folder next to it).",
  errors: {
    unsupported_format: () => "unsupported package version",
    unknown_kdf: () => "unknown KDF",
    read_failed: (file) => `couldn't read ${file}`,
    manifest_missing: () => "manifest missing",
    bad_chunk_id: () => "invalid chunk id",
    chat_locked: () => "chat not unlocked",
    media_truncated: () => "media package is truncated",
  },
};

export const shellMessages: Record<Lang, ShellMessages> = { "pt-BR": pt, en };

export const useShellT = makeT(shellMessages);

/** Error with a stable code: the text is translated when displayed. The message
 * already comes out in the requested language (for those that only read `.message`, like the viewer). */
export class ShellError extends Error {
  constructor(
    readonly code: ErrorCode,
    readonly detail = "",
    lang: Lang = "pt-BR",
  ) {
    super(shellMessages[lang].errors[code](detail));
  }

  /** The same error, with the message in another language. */
  in(lang: Lang): ShellError {
    return new ShellError(this.code, this.detail, lang);
  }
}

/** Text of an error in the current language. */
export function errorText(err: unknown, lang: Lang): string {
  if (err instanceof ShellError) return shellMessages[lang].errors[err.code](err.detail);
  return err instanceof Error ? err.message : String(err);
}
