import { createContext, useContext, useSyncExternalStore } from "react";
import type { Message } from "./types";

/** Whether the message list is scrolling, as a small store: only the components
 * that care (full-resolution images) re-render when it changes. */
export class ScrollWatch {
  private scrolling = false;
  private timer: ReturnType<typeof setTimeout> | undefined;
  private listeners = new Set<() => void>();

  subscribe = (cb: () => void) => {
    this.listeners.add(cb);
    return () => void this.listeners.delete(cb);
  };

  get = () => this.scrolling;

  /** Called on every scroll event; "stopped" 150 ms after the last one. */
  touch() {
    clearTimeout(this.timer);
    this.timer = setTimeout(() => this.set(false), 150);
    this.set(true);
  }

  private set(v: boolean) {
    if (v === this.scrolling) return;
    this.scrolling = v;
    this.listeners.forEach((l) => l());
  }
}

/** State shared by the bubbles without going through props (the rows are memoized). */
export type ViewerCtx = {
  scroll: ScrollWatch;
  openMedia: (m: Message) => void;
  /** Asks for confirmation before opening a link from the message. */
  openLink: (url: string) => void;
};

export const ViewerContext = createContext<ViewerCtx>({ scroll: new ScrollWatch(), openMedia: () => {}, openLink: () => {} });
export const useViewer = () => useContext(ViewerContext);

/** The list is scrolling: full-resolution images wait until it stops. */
export function useScrolling(): boolean {
  const { scroll } = useViewer();
  return useSyncExternalStore(scroll.subscribe, scroll.get);
}
