import type { CSSProperties } from "react";
import type { Chat } from "./types";
import { avatarHue, chatTitle } from "./format";
import { Icon } from "./icons";

/** No profile pictures (the backup doesn't include them): an initial or icon over
 * a light tint, stable per JID, like WhatsApp's default avatars. */
export default function Avatar({ chat, size = 49 }: { chat: Chat; size?: number }) {
  const title = chatTitle(chat);
  const letter = /^[+~\d]/.test(title) ? null : ([...title.trim()][0]?.toUpperCase() ?? null);
  const iconSize = Math.round(size * 0.55);
  const content =
    chat.kind === "group" ? (
      <Icon name="group" size={iconSize} fill strokeWidth={1.5} />
    ) : chat.kind === "broadcast" || chat.kind === "channel" ? (
      <Icon name="broadcast" size={iconSize} strokeWidth={1.75} />
    ) : letter ? (
      letter
    ) : (
      <Icon name="account" size={iconSize} fill strokeWidth={1.5} />
    );
  const style = { "--h": avatarHue(chat.jid), width: size, height: size, fontSize: Math.round(size * 0.4) } as CSSProperties;
  return (
    <span className="avatar" style={style}>
      {content}
    </span>
  );
}
