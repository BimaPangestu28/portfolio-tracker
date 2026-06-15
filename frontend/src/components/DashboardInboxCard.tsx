import { Link } from "react-router-dom";
import { useInbox } from "../api/hooks";

const MAX_ROWS = 5;

export function DashboardInboxCard() {
  const inbox = useInbox();
  const rows = (inbox.data ?? []).slice(0, MAX_ROWS);

  return (
    <div className="card">
      <div className="card-head flex items-center justify-between">
        <div className="card-title">Inbox</div>
        <Link to="/chat" className="text-sm text-primary hover:underline">Tanya Noah →</Link>
      </div>
      <div className="card-pad space-y-1" style={{ paddingTop: 14 }}>
        {rows.length === 0 && <p className="text-sm text-muted-foreground">Inbox kosong.</p>}
        {rows.map((i) => (
          <div key={i.id} className="flex items-center gap-2 text-sm">
            <span className="flex-1 truncate">{i.content}</span>
          </div>
        ))}
      </div>
    </div>
  );
}
