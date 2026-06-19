import { useState } from "react";
import { Link } from "react-router-dom";
import { useNewsDates, useNewsToday } from "../api/hooks";
import NewsDigest from "../components/NewsDigest";
import { QueryState } from "../components/QueryState";

const PAGE = 30;

const formatDate = (iso: string) =>
  new Date(`${iso}T00:00:00`).toLocaleDateString("id-ID", { day: "numeric", month: "short", year: "numeric" });

export default function NewsPage() {
  const q = useNewsToday();
  const [limit, setLimit] = useState(PAGE);
  const dates = useNewsDates(limit);

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 32 }}>
      <QueryState isLoading={q.isLoading} error={q.error}>
        {q.data && !q.data.available ? (
          <p style={{ color: "hsl(var(--muted-foreground))" }}>
            Digest berita hari ini belum siap. Cek lagi nanti pagi ya.
          </p>
        ) : q.data ? (
          <NewsDigest date={q.data.date ?? ""} articles={q.data.articles} quiz={q.data.quiz} />
        ) : null}
      </QueryState>

      {dates.data && dates.data.length > 0 && (
        <section style={{ display: "flex", flexDirection: "column", gap: 8 }}>
          <h2 style={{ fontSize: 16, fontWeight: 600 }}>Arsip</h2>
          {dates.data.map((d) => (
            <Link
              key={d.date}
              to={`/news/${d.date}`}
              className="card card-pad"
              style={{ textDecoration: "none", display: "flex", justifyContent: "space-between", gap: 8 }}
            >
              <span>{formatDate(d.date)}</span>
              <span style={{ color: "hsl(var(--muted-foreground))", fontSize: 13 }}>
                {d.article_count} artikel
              </span>
            </Link>
          ))}
          {dates.data.length >= limit && (
            <button className="btn btn-secondary" onClick={() => setLimit((l) => l + PAGE)}>
              Muat lebih banyak
            </button>
          )}
        </section>
      )}
    </div>
  );
}
