import { memo } from "react";
import { Virtuoso } from "react-virtuoso";
import type { Chat } from "../api";
import Avatar from "./Avatar";
import { chatTitle, formatListTime, personLabel, previewText } from "./format";

type Props = {
  chats: Chat[];
  selectedId: number | null;
  onSelect: (chat: Chat) => void;
  archivedCount: number;
  showingArchived: boolean;
  onToggleArchived: () => void;
};

export default function ChatList({ chats, selectedId, onSelect, archivedCount, showingArchived, onToggleArchived }: Props) {
  return (
    <div className="chat-list">
      {(archivedCount > 0 || showingArchived) && (
        <button className="archived-row" onClick={onToggleArchived}>
          <span className="archived-icon">{showingArchived ? "←" : "🗄️"}</span>
          <span>{showingArchived ? "Voltar às conversas" : "Arquivadas"}</span>
          {!showingArchived && <span className="count">{archivedCount}</span>}
        </button>
      )}
      <Virtuoso
        className="chat-list-scroll"
        data={chats}
        computeItemKey={(_, c) => c.id}
        itemContent={(_, c) => <ChatItem chat={c} selected={c.id === selectedId} onSelect={onSelect} />}
      />
    </div>
  );
}

const ChatItem = memo(function ChatItem({
  chat,
  selected,
  onSelect,
}: {
  chat: Chat;
  selected: boolean;
  onSelect: (c: Chat) => void;
}) {
  const last = chat.last;
  const prefix = !last
    ? ""
    : last.fromMe && last.kind !== "system"
      ? "Você: "
      : chat.kind === "group" && last.sender
        ? `${personLabel(last.sender)}: `
        : "";
  return (
    <button className={`chat-item ${selected ? "selected" : ""}`} onClick={() => onSelect(chat)}>
      <Avatar chat={chat} />
      <span className="chat-item-body">
        <span className="chat-item-top">
          <span className="chat-item-title">{chatTitle(chat)}</span>
          {chat.lastMessageAt != null && <span className="chat-item-time">{formatListTime(chat.lastMessageAt)}</span>}
        </span>
        <span className="chat-item-preview">{last ? prefix + previewText(last.kind, last.text) : ""}</span>
      </span>
    </button>
  );
});
