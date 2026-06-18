import { useState, useRef, useEffect } from "react";
import { Send, Sparkles, Wallet } from "lucide-react";
import { useChatHistory, useSendChat, useWhatsappStatus } from "../api/hooks";
import { QueryState } from "../components/QueryState";
import { MarkdownMessage } from "../components/MarkdownMessage";

const SUGGESTIONS = [
  "Apa agenda saya hari ini?",
  "Ingetin meeting jam 3 sore",
  "Catat todo: bayar internet",
  "Berapa net worth saya?",
];

export default function ChatPage() {
  const history = useChatHistory();
  const sendChat = useSendChat();
  const waStatus = useWhatsappStatus();
  const waConnected = waStatus.data?.status === "connected";
  const [input, setInput] = useState("");
  const scrollRef = useRef<HTMLDivElement>(null);

  const msgs = history.data ?? [];

  // Autoscroll on new messages
  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [msgs, sendChat.isPending]);

  const handleSend = (e?: React.FormEvent) => {
    e?.preventDefault();
    const trimmed = input.trim();
    if (!trimmed) return;
    sendChat.mutate(trimmed, {
      onSuccess: () => setInput(""),
    });
  };

  const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  };

  return (
    <div style={{ display: "flex", flexDirection: "column", height: "calc(100vh - 64px - 48px)", maxHeight: 760 }}>
      {/* Header */}
      <div className="flex items-center justify-between" style={{ marginBottom: 12 }}>
        <div>
          <h1 className="t-h1">Noah</h1>
          <div className="t-sm t-muted">Asisten pribadi kamu</div>
        </div>
        <span className={waConnected ? "badge badge-gain" : "badge"}>
          <span className="badge-dot" style={{ background: "currentColor" }} />
          {waConnected ? "Sinkron dengan WhatsApp" : "WhatsApp tidak terhubung"}
        </span>
      </div>

      {/* Chat card */}
      <div className="card" style={{ display: "flex", flexDirection: "column", flex: 1, overflow: "hidden" }}>
        <QueryState isLoading={history.isLoading} error={history.error}>
          {/* Messages scroll area */}
          <div
            ref={scrollRef}
            style={{ flex: 1, overflowY: "auto", padding: 22, display: "flex", flexDirection: "column", gap: 16 }}
          >
            {msgs.length === 0 && !sendChat.isPending && (
              <p className="t-sm t-muted" style={{ margin: "auto", textAlign: "center" }}>
                No messages yet. Ask about your portfolio!
              </p>
            )}
            {msgs.map((msg) => {
              const isUser = msg.role === "user";
              return (
                <div
                  key={msg.id}
                  style={{
                    display: "flex",
                    flexDirection: isUser ? "row-reverse" : "row",
                    alignItems: "flex-start",
                    gap: 12,
                  }}
                >
                  {/* Avatar */}
                  <span
                    style={{
                      width: 30,
                      height: 30,
                      borderRadius: 8,
                      flexShrink: 0,
                      display: "grid",
                      placeItems: "center",
                      background: isUser
                        ? "hsl(var(--muted))"
                        : "linear-gradient(150deg, hsl(var(--primary)), hsl(262 83% 64%))",
                      color: isUser ? "hsl(var(--muted-foreground))" : "#fff",
                    }}
                    aria-hidden="true"
                  >
                    {isUser ? <Wallet size={16} /> : <Sparkles size={16} />}
                  </span>
                  {/* Bubble */}
                  <div
                    style={{
                      maxWidth: "76%",
                      padding: "11px 14px",
                      borderRadius: 14,
                      fontSize: 13.5,
                      lineHeight: 1.55,
                      background: isUser ? "hsl(var(--primary))" : "hsl(var(--muted))",
                      color: isUser ? "hsl(var(--primary-foreground))" : "hsl(var(--foreground))",
                      borderTopRightRadius: isUser ? 4 : 14,
                      borderTopLeftRadius: isUser ? 14 : 4,
                    }}
                  >
                    {isUser ? msg.content : <MarkdownMessage content={msg.content} />}
                  </div>
                </div>
              );
            })}

            {/* Thinking indicator */}
            {sendChat.isPending && (
              <div style={{ display: "flex", gap: 12, alignItems: "flex-start" }}>
                <span
                  style={{
                    width: 30,
                    height: 30,
                    borderRadius: 8,
                    display: "grid",
                    placeItems: "center",
                    background: "linear-gradient(150deg, hsl(var(--primary)), hsl(262 83% 64%))",
                    color: "#fff",
                  }}
                  aria-hidden="true"
                >
                  <Sparkles size={16} />
                </span>
                <div
                  style={{
                    padding: "13px 16px",
                    borderRadius: 14,
                    borderTopLeftRadius: 4,
                    background: "hsl(var(--muted))",
                  }}
                  aria-label="Sedang berpikir…"
                >
                  <div style={{ display: "flex", gap: 4 }}>
                    <span className="think-dot" />
                    <span className="think-dot" style={{ animationDelay: "0.15s" }} />
                    <span className="think-dot" style={{ animationDelay: "0.3s" }} />
                  </div>
                </div>
              </div>
            )}
          </div>
        </QueryState>

        {/* Composer */}
        <div style={{ borderTop: "1px solid hsl(var(--border))", padding: "14px 18px" }}>
          {/* Suggestion chips */}
          <div style={{ display: "flex", gap: 8, marginBottom: 10, flexWrap: "wrap" }}>
            {SUGGESTIONS.map((s) => (
              <button
                key={s}
                type="button"
                className="chip"
                onClick={() => setInput(s)}
                aria-label={`Saran: ${s}`}
                style={{ maxWidth: "100%", height: "auto", minHeight: 30, whiteSpace: "normal", textAlign: "left" }}
              >
                {s}
              </button>
            ))}
          </div>
          {/* Input row */}
          <form onSubmit={handleSend} style={{ display: "flex", gap: 8, alignItems: "flex-end" }}>
            <textarea
              className="input"
              rows={1}
              placeholder="Tanya tentang portofolio…"
              value={input}
              onChange={(e) => setInput(e.target.value)}
              onKeyDown={handleKeyDown}
              disabled={sendChat.isPending}
              style={{ flex: 1, resize: "none" }}
              aria-label="Chat message"
            />
            <button
              type="submit"
              className="btn btn-primary"
              disabled={sendChat.isPending || !input.trim()}
              style={{ width: 42, padding: 0, height: 42, flexShrink: 0 }}
              aria-label="Kirim pesan"
            >
              <Send size={17} />
            </button>
          </form>
          {sendChat.error && (
            <div className="t-sm loss" style={{ marginTop: 8 }}>
              {(sendChat.error as Error).message}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
