import { useState } from "react";
import { toast } from "sonner";
import {
  useCsConversations,
  useCsEscalations,
  useHandleEscalation,
  useCsTranscript,
  useResolveConversation,
  useReplyConversation,
} from "@/api/hooks";

export default function CsInboxPage() {
  const convos = useCsConversations();
  const escalations = useCsEscalations();
  const handle = useHandleEscalation();
  const resolve = useResolveConversation();
  const reply = useReplyConversation();
  const [selected, setSelected] = useState<number | null>(null);
  const [replyText, setReplyText] = useState("");
  const transcript = useCsTranscript(selected);

  const convoList = convos.data ?? [];
  const escList = escalations.data ?? [];

  return (
    <div>
      <div style={{ marginBottom: 18 }}>
        <h1 className="t-h1">CS Inbox</h1>
        <div className="t-sm t-muted" style={{ marginTop: 2 }}>Percakapan, eskalasi, &amp; transkrip</div>
      </div>

      <div className="lay-side" style={{ gap: 16, alignItems: "start" }}>
        {/* Left column: escalations + conversations */}
        <div style={{ display: "flex", flexDirection: "column", gap: 16 }}>
          {/* Escalations */}
          <div className="card">
            <div className="card-head">
              <div className="card-title">Perlu manusia ({escList.length})</div>
            </div>
            <div style={{ padding: "8px 0" }}>
              {escList.length === 0 ? (
                <div className="t-sm t-muted" style={{ padding: "12px 16px" }}>Tidak ada eskalasi.</div>
              ) : (
                escList.map((e) => (
                  <div key={e.id} style={{ padding: "10px 16px", borderBottom: "1px solid hsl(var(--border))" }}>
                    <div className="t-sm" style={{ fontWeight: 500, marginBottom: 2 }}>{e.reason}</div>
                    <div className="t-sm t-muted" style={{ marginBottom: 6 }}>{e.summary}</div>
                    <div className="flex items-center" style={{ gap: 8 }}>
                      <button
                        type="button"
                        className="btn btn-outline btn-sm"
                        onClick={() => setSelected(e.conversation_id)}
                      >
                        Lihat
                      </button>
                      <button
                        type="button"
                        className="btn btn-primary btn-sm"
                        disabled={handle.isPending}
                        onClick={() =>
                          handle.mutate(e.id, {
                            onSuccess: () => toast.success("Ditandai ditangani"),
                            onError: (err) => toast.error((err as Error).message),
                          })
                        }
                      >
                        Tangani
                      </button>
                    </div>
                  </div>
                ))
              )}
            </div>
          </div>

          {/* Conversation list */}
          <div className="card">
            <div className="card-head">
              <div className="card-title">Percakapan</div>
            </div>
            <div style={{ padding: "8px 0" }}>
              {convoList.length === 0 ? (
                <div className="t-sm t-muted" style={{ padding: "12px 16px" }}>Belum ada percakapan.</div>
              ) : (
                convoList.map((c) => (
                  <button
                    key={c.id}
                    type="button"
                    style={{
                      width: "100%",
                      textAlign: "left",
                      padding: "10px 16px",
                      display: "flex",
                      alignItems: "center",
                      justifyContent: "space-between",
                      gap: 8,
                      background: selected === c.id ? "hsl(var(--muted))" : "transparent",
                      border: "none",
                      cursor: "pointer",
                      borderBottom: "1px solid hsl(var(--border))",
                    }}
                    onClick={() => setSelected(c.id)}
                  >
                    <span className="t-sm" style={{ fontWeight: 500 }}>{c.visitor_name ?? `#${c.id}`}</span>
                    <span className="t-sm t-muted">{c.status}</span>
                  </button>
                ))
              )}
            </div>
          </div>
        </div>

        {/* Right column: transcript */}
        <div className="card">
          <div className="card-head" style={{ justifyContent: "space-between" }}>
            <div className="card-title">
              Transkrip {selected != null ? `#${selected}` : ""}
            </div>
            {selected != null && (
              <button
                type="button"
                className="btn btn-outline btn-sm"
                disabled={resolve.isPending}
                onClick={() =>
                  resolve.mutate(selected, {
                    onSuccess: () => toast.success("Diselesaikan"),
                    onError: (e) => toast.error((e as Error).message),
                  })
                }
              >
                Tandai selesai
              </button>
            )}
          </div>
          <div style={{ padding: 16, display: "flex", flexDirection: "column", gap: 8, minHeight: 200 }}>
            {selected == null ? (
              <div className="t-sm t-muted">Pilih percakapan untuk melihat transkrip.</div>
            ) : (transcript.data ?? []).length === 0 ? (
              <div className="t-sm t-muted">Tidak ada pesan.</div>
            ) : (
              (transcript.data ?? []).map((m) => (
                <div
                  key={m.id}
                  style={{
                    alignSelf: m.role === "user" ? "flex-start" : "flex-end",
                    maxWidth: "80%",
                    background: m.role === "user" ? "hsl(var(--muted))" : "hsl(var(--primary))",
                    color: m.role === "user" ? "hsl(var(--foreground))" : "hsl(var(--primary-foreground))",
                    padding: "8px 10px",
                    borderRadius: "var(--radius)",
                    whiteSpace: "pre-wrap",
                  }}
                >
                  <div className="t-sm">{m.content}</div>
                </div>
              ))
            )}
          </div>

          {selected != null && (
            <div style={{ padding: "0 16px 16px", display: "flex", gap: 8 }}>
              <textarea
                className="t-sm"
                rows={2}
                value={replyText}
                onChange={(e) => setReplyText(e.target.value)}
                placeholder="Balas sebagai owner…"
                style={{
                  flex: 1,
                  resize: "vertical",
                  padding: "8px 10px",
                  borderRadius: "var(--radius)",
                  border: "1px solid hsl(var(--border))",
                  background: "hsl(var(--background))",
                  color: "hsl(var(--foreground))",
                  fontFamily: "inherit",
                }}
              />
              <button
                type="button"
                className="btn btn-primary btn-sm"
                disabled={reply.isPending || replyText.trim() === ""}
                onClick={() => {
                  const text = replyText.trim();
                  if (!text) return;
                  reply.mutate(
                    { id: selected, text },
                    {
                      onSuccess: () => {
                        toast.success("Balasan terkirim");
                        setReplyText("");
                        transcript.refetch();
                      },
                      onError: (err) => toast.error((err as Error).message),
                    },
                  );
                }}
              >
                Kirim
              </button>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
