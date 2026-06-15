import { useState } from "react";
import { Check, RotateCcw } from "lucide-react";
import { toast } from "sonner";
import { useInbox, useResolveInbox, useUnresolveInbox } from "../../api/hooks";

const STATUSES: { key: string; label: string }[] = [
  { key: "pending", label: "Pending" },
  { key: "sorted", label: "Selesai" },
  { key: "all", label: "Semua" },
];

export function InboxTab() {
  const [status, setStatus] = useState("pending");
  const inbox = useInbox(status);
  const resolve = useResolveInbox();
  const unresolve = useUnresolveInbox();

  const rows = inbox.data ?? [];

  return (
    <div className="flex col gap-3">
      <div className="ptabs" role="group" aria-label="Filter status">
        {STATUSES.map((st) => (
          <button key={st.key} className={`ptab${status === st.key ? " active" : ""}`} onClick={() => setStatus(st.key)}>{st.label}</button>
        ))}
      </div>

      {rows.length === 0 && <p className="text-sm text-muted-foreground">Inbox kosong.</p>}
      {rows.map((i) => (
        <div key={i.id} className="flex items-center gap-2 text-sm">
          <span className="flex-1 truncate">{i.content}</span>
          {i.status === "pending" ? (
            <button type="button" aria-label={`Selesaikan ${i.content}`} className="pt-icon-btn shrink-0" disabled={resolve.isPending}
              onClick={() => resolve.mutate(i.id, { onSuccess: () => toast.success("Inbox ditangani"), onError: (e) => toast.error((e as Error).message) })}>
              <Check size={15} />
            </button>
          ) : (
            <button type="button" aria-label={`Buka lagi ${i.content}`} className="pt-icon-btn shrink-0" disabled={unresolve.isPending}
              onClick={() => unresolve.mutate(i.id, { onSuccess: () => toast.success("Dikembalikan ke pending"), onError: (e) => toast.error((e as Error).message) })}>
              <RotateCcw size={15} />
            </button>
          )}
        </div>
      ))}
    </div>
  );
}
