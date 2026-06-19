import { Link, useParams } from "react-router-dom";
import { useNewsDigest } from "../api/hooks";
import NewsDigest from "../components/NewsDigest";
import { QueryState } from "../components/QueryState";

export default function NewsDatePage() {
  const { date } = useParams<{ date: string }>();
  const q = useNewsDigest(date);
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 16 }}>
      <Link to="/news" style={{ fontSize: 13, color: "hsl(var(--muted-foreground))", textDecoration: "none" }}>
        ← Kembali
      </Link>
      <QueryState isLoading={q.isLoading} error={q.error}>
        {q.data && !q.data.available ? (
          <p style={{ color: "hsl(var(--muted-foreground))" }}>Tidak ada digest untuk tanggal ini.</p>
        ) : q.data ? (
          <NewsDigest date={q.data.date ?? date ?? ""} articles={q.data.articles} quiz={q.data.quiz} />
        ) : null}
      </QueryState>
    </div>
  );
}
