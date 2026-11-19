import { createContext, useContext } from "react";
import type { Message } from "./types";

/** State shared by the bubbles without going through props (the rows are memoized). */
export type ViewerCtx = {
  /** The list is scrolling: full-resolution images wait until it stops. */
  scrolling: boolean;
  openMedia: (m: Message) => void;
  /** Asks for confirmation before opening a link from the message. */
  openLink: (url: string) => void;
};

export const ViewerContext = createContext<ViewerCtx>({ scrolling: false, openMedia: () => {}, openLink: () => {} });
export const useViewer = () => useContext(ViewerContext);
