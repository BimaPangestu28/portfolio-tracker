import { useNewsToday } from "../api/hooks";
import NewsDigest from "../components/NewsDigest";
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
        <NewsDigest date={q.data.date ?? ""} articles={q.data.articles} quiz={q.data.quiz} />
      ) : null}
    </QueryState>
  );
}
