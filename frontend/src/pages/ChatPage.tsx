import { useState } from "react";
import { useChatHistory, useSendChat } from "../api/hooks";
import { QueryState } from "../components/QueryState";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { cn } from "@/lib/utils";

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
        <Card className="flex min-h-48 flex-col gap-2 p-4">
          {(history.data ?? []).length === 0 && (
            <p className="text-sm text-muted-foreground">No messages yet. Ask about your portfolio!</p>
          )}
          {(history.data ?? []).map((msg) => (
            <div key={msg.id} className={cn("flex", msg.role === "user" ? "justify-end" : "justify-start")}>
              <div
                className={cn(
                  "max-w-xs rounded-lg px-3 py-2 text-sm lg:max-w-md",
                  msg.role === "user" ? "bg-primary text-primary-foreground" : "bg-muted text-foreground",
                )}
              >
                {msg.content}
              </div>
            </div>
          ))}
          {sendChat.isPending && (
            <div className="flex justify-start">
              <div className="rounded-lg bg-muted px-3 py-2 text-sm italic text-muted-foreground">thinking…</div>
            </div>
          )}
        </Card>
      </QueryState>

      <form onSubmit={handleSend} className="flex gap-2">
        <Input
          aria-label="Chat message"
          className="flex-1"
          placeholder="Ask about your portfolio…"
          value={input}
          onChange={(e) => setInput(e.target.value)}
          disabled={sendChat.isPending}
        />
        <Button type="submit" disabled={sendChat.isPending || !input.trim()}>
          Send
        </Button>
      </form>

      {sendChat.error && <div className="text-sm text-destructive">{(sendChat.error as Error).message}</div>}
    </div>
  );
}
