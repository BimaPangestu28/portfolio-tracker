import { Link } from "react-router-dom";
import { Check } from "lucide-react";
import { toast } from "sonner";
import { useInbox, useResolveInbox } from "../api/hooks";

const MAX_ROWS = 5;

export function DashboardInboxCard() {
  const inbox = useInbox();
  const resolveInbox = useResolveInbox();
  const rows = (inbox.data ?? []).slice(0, MAX_ROWS);

  function handleResolve(id: number) {
    resolveInbox.mutate(id, {
      onSuccess: () => toast.success("Inbox ditangani"),
      onError: (err) => toast.error((err as Error).message),
    });
  }

  return (
    <div className="card">
      <div className="card-head flex items-center justify-between">
        <div className="card-title">Inbox</div>
        <Link to="/tugas" className="text-sm text-primary hover:underline">Lihat semua →</Link>
      </div>
      <div className="card-pad space-y-1" style={{ paddingTop: 14 }}>
        {rows.length === 0 && <p className="text-sm text-muted-foreground">Inbox kosong.</p>}
        {rows.map((i) => (
          <div key={i.id} className="flex items-center gap-2 text-sm">
            <span className="flex-1 truncate">{i.content}</span>
            <button
              type="button"
              aria-label={`Selesaikan ${i.content}`}
              className="pt-icon-btn shrink-0"
              disabled={resolveInbox.isPending}
              onClick={() => handleResolve(i.id)}
            >
              <Check size={15} />
            </button>
          </div>
        ))}
      </div>
    </div>
  );
}
