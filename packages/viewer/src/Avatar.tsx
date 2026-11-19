import type { Chat } from "./types";
import { chatTitle, senderColor } from "./format";

/** Sem fotos de perfil (o backup não traz): iniciais ou ícone, cor estável por JID. */
export default function Avatar({ chat }: { chat: Chat }) {
  const icon = chat.kind === "group" ? "👥" : chat.kind === "broadcast" ? "📢" : chat.kind === "channel" ? "📣" : null;
  const title = chatTitle(chat);
  const initials = /^\+?\d/.test(title)
    ? null
    : title
        .split(/\s+/)
        .slice(0, 2)
        .map((w) => [...w][0]?.toUpperCase() ?? "")
        .join("");
  return (
    <span className="avatar" style={{ background: senderColor(chat.jid) }}>
      {icon ?? initials ?? "👤"}
    </span>
  );
}
