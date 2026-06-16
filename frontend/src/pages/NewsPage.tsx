import { useNewsToday } from "../api/hooks";
import NewsQuiz from "../components/NewsQuiz";
import { QueryState } from "../components/QueryState";

export default function NewsPage() {
  const q = useNewsToday();
  return (
    <QueryState isLoading={q.isLoading} error={q.error}>
      {q.data && !q.data.available ? (
        <p style={{ color: "hsl(var(--muted-foreground))" }}>
          Digest berita hari ini belum siap. Cek lagi nanti pagi ya.
        </p>
      ) : q.data ? (
        <div style={{ display: "flex", flexDirection: "column", gap: 24 }}>
          <header>
            <h1 style={{ fontSize: 20, fontWeight: 600, letterSpacing: "-0.015em" }}>Bacaan pagi</h1>
            <p style={{ fontSize: 13, color: "hsl(var(--muted-foreground))", marginTop: 2 }}>{q.data.date}</p>
          </header>
          {q.data.articles.map((a) => (
            <article key={a.position} className="card card-pad">
              {a.image_url != null && (
                <img
                  src={a.image_url}
                  alt=""
                  loading="lazy"
                  className="w-full h-40 object-cover rounded-md mb-3"
                  onError={(e) => { e.currentTarget.style.display = "none"; }}
                />
              )}
              <div style={{ display: "flex", alignItems: "center", gap: 8, flexWrap: "wrap", marginBottom: 8 }}>
                <span className="badge badge-neutral">{a.source}</span>
                {a.read_minutes != null && (
                  <span style={{ fontSize: 12, color: "hsl(var(--muted-foreground))" }}>
                    ⏱ {a.read_minutes} mnt baca
                  </span>
                )}
              </div>
              <a
                href={a.url}
                target="_blank"
                rel="noreferrer"
                style={{ fontSize: 16, fontWeight: 600, textDecoration: "none" }}
                onMouseEnter={(e) => { (e.target as HTMLAnchorElement).style.textDecoration = "underline"; }}
                onMouseLeave={(e) => { (e.target as HTMLAnchorElement).style.textDecoration = "none"; }}
              >
                {a.title}
              </a>
              <p style={{ marginTop: 8 }}>{a.summary}</p>
              {a.key_points.length > 0 && (
                <ul style={{ marginTop: 8, paddingLeft: 20, fontSize: 13 }}>
                  {a.key_points.map((k, i) => (
                    <li key={i}>{k}</li>
                  ))}
                </ul>
              )}
            </article>
          ))}
          <NewsQuiz questions={q.data.quiz} date={q.data.date ?? ""} />
        </div>
      ) : null}
    </QueryState>
  );
}
