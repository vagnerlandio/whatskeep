import { createContext, useContext } from "react";
import type { Message } from "../api";

/** Estado compartilhado pelas bolhas sem passar por props (as linhas são memo). */
export type ViewerCtx = {
  /** A lista está rolando: imagens em resolução total esperam parar. */
  scrolling: boolean;
  openMedia: (m: Message) => void;
};

export const ViewerContext = createContext<ViewerCtx>({ scrolling: false, openMedia: () => {} });
export const useViewer = () => useContext(ViewerContext);
