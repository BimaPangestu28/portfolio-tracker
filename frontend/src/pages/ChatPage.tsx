import { useState } from "react";
import { useChatHistory, useSendChat } from "../api/hooks";
import { QueryState } from "../components/QueryState";

export default function ChatPage() {
  const history = useChatHistory();
  const sendChat = useSendChat();
  const [input, setInput] = useState("");

  const handleSend = (e: React.FormEvent) => {
    e.preventDefault();
    const trimmed = input.trim();
    if (!trimmed) return;
    sendChat.mutate(trimmed, {
      onSuccess: () => setInput(""),
    });
  };

  return (
    <div className="flex flex-col space-y-4">
      <h1 className="text-xl font-semibold">Chat</h1>

      <QueryState isLoading={history.isLoading} error={history.error}>
        <div className="flex flex-col gap-2 rounded border bg-white p-4 min-h-48">
          {(history.data ?? []).length === 0 && (
            <p className="text-gray-400 text-sm">No messages yet. Ask about your portfolio!</p>
          )}
          {(history.data ?? []).map((msg) => (
            <div
              key={msg.id}
              className={`flex ${msg.role === "user" ? "justify-end" : "justify-start"}`}
            >
              <div
                className={`max-w-xs rounded-lg px-3 py-2 text-sm lg:max-w-md ${
                  msg.role === "user"
                    ? "bg-blue-600 text-white"
                    : "bg-gray-100 text-gray-900"
                }`}
              >
                {msg.content}
              </div>
            </div>
          ))}
          {sendChat.isPending && (
            <div className="flex justify-start">
              <div className="rounded-lg bg-gray-100 px-3 py-2 text-sm text-gray-500 italic">
                thinking…
              </div>
            </div>
          )}
        </div>
      </QueryState>

      <form onSubmit={handleSend} className="flex gap-2">
        <input
          aria-label="Chat message"
          className="flex-1 rounded border px-3 py-2 text-sm"
          placeholder="Ask about your portfolio…"
          value={input}
          onChange={(e) => setInput(e.target.value)}
          disabled={sendChat.isPending}
        />
        <button
          type="submit"
          className="rounded bg-blue-600 px-4 py-2 text-sm text-white disabled:opacity-50"
          disabled={sendChat.isPending || !input.trim()}
        >
          Send
        </button>
      </form>

      {sendChat.error && (
        <div className="text-sm text-red-600">
          {(sendChat.error as Error).message}
        </div>
      )}
    </div>
  );
}
