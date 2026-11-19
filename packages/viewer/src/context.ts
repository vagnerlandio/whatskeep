import { createContext, useContext } from "react";
import type { Message } from "./types";

/** Estado compartilhado pelas bolhas sem passar por props (as linhas são memo). */
export type ViewerCtx = {
  /** A lista está rolando: imagens em resolução total esperam parar. */
  scrolling: boolean;
  openMedia: (m: Message) => void;
  /** Pede confirmação antes de abrir um link da mensagem. */
  openLink: (url: string) => void;
};

export const ViewerContext = createContext<ViewerCtx>({ scrolling: false, openMedia: () => {}, openLink: () => {} });
export const useViewer = () => useContext(ViewerContext);
