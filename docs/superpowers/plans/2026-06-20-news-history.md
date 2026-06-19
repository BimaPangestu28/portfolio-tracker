# News History Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let the user browse previously scraped news digests — a paginated date list on `/news`, each opening a detail view identical to today's (articles + quiz).

**Architecture:** Two new read-only backend endpoints (`GET /news/dates`, `GET /news/digest/:date`) that reuse the existing `repo::news` date-scoped queries; `today()` is refactored to share a `load_digest` core. The frontend extracts a shared `NewsDigest` component, adds an "Arsip" list to `NewsPage`, and a `NewsDatePage` detail route.

**Tech Stack:** Rust (axum, sqlx, SQLite), React + TypeScript (react-router-dom, @tanstack/react-query, zod, vitest/msw).

## Global Constraints

- **Never run `cargo fmt`** — it rewrites hundreds of unrelated files.
- **Backend tests run with `cargo test`** (from `backend/`), NOT `cargo test --lib` (bin-only crate; `--lib` errors). Run a single test with `cargo test <test_name>`.
- **Frontend tests:** `npm test` (= `vitest run`) from `frontend/`; single file `npx vitest run <path>`.
- Match existing code style: inline `style={{…}}` objects, Indonesian user-facing copy, no new dependencies.
- Response shape of `GET /news/today` (`TodayDto`) must remain unchanged.
- Work happens in the worktree `.worktrees/news-history` on branch `feat/news-history`.

---

### Task 1: Repo — paginated digest date list

**Files:**
- Modify: `backend/src/repo/news.rs` (add `DateRow` after `QuizRow`; add `dates()` after `quiz()`; add a test in the existing `#[cfg(test)] mod tests`)

**Interfaces:**
- Produces: `pub struct DateRow { pub digest_date: String, pub created_at: String, pub article_count: i64 }` and `pub async fn dates(db: &Db, limit: i64, offset: i64) -> anyhow::Result<Vec<DateRow>>`.

- [ ] **Step 1: Write the failing test**

Add to the `mod tests` block in `backend/src/repo/news.rs`:

```rust
    #[tokio::test]
    async fn dates_lists_newest_first_with_counts() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let art = |pos: i64| NewArticle {
            position: pos, title: "t".into(), url: format!("https://ex.com/{pos}"),
            source: "HN".into(), score: 1, summary: "s".into(),
            key_points_json: "[]".into(), image_url: None, read_minutes: None,
        };
        insert(&db, "2026-06-18", "2026-06-18T00:00:00Z", &[art(0)], &[]).await.unwrap();
        insert(&db, "2026-06-19", "2026-06-19T00:00:00Z", &[art(0), art(1)], &[]).await.unwrap();

        let all = dates(&db, 30, 0).await.unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].digest_date, "2026-06-19");
        assert_eq!(all[0].article_count, 2);
        assert_eq!(all[1].digest_date, "2026-06-18");
        assert_eq!(all[1].article_count, 1);

        // pagination: limit 1, offset 1 → the second-newest only
        let page = dates(&db, 1, 1).await.unwrap();
        assert_eq!(page.len(), 1);
        assert_eq!(page[0].digest_date, "2026-06-18");
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd backend && cargo test dates_lists_newest_first_with_counts`
Expected: FAIL to compile — `cannot find function dates` / `cannot find type DateRow`.

- [ ] **Step 3: Write minimal implementation**

Add the struct after `QuizRow` (around line 29) in `backend/src/repo/news.rs`:

```rust
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct DateRow {
    pub digest_date: String,
    pub created_at: String,
    pub article_count: i64,
}
```

Add the function after `quiz()` (around line 110):

```rust
/// Distinct digest dates, newest first, with their article counts. Paginated.
pub async fn dates(db: &Db, limit: i64, offset: i64) -> anyhow::Result<Vec<DateRow>> {
    Ok(sqlx::query_as(
        "SELECT d.digest_date, d.created_at, COUNT(a.position) AS article_count
         FROM news_digest d
         LEFT JOIN news_article a ON a.digest_date = d.digest_date
         GROUP BY d.digest_date, d.created_at
         ORDER BY d.digest_date DESC
         LIMIT ? OFFSET ?")
        .bind(limit).bind(offset).fetch_all(db).await?)
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd backend && cargo test dates_lists_newest_first_with_counts`
Expected: PASS (1 passed).

- [ ] **Step 5: Commit**

```bash
git add backend/src/repo/news.rs
git commit -m "feat(news): repo dates() — paginated digest date list with counts"
```

---

### Task 2: API — refactor today() into load_digest + add digest_by_date

**Files:**
- Modify: `backend/src/api/news.rs` (add `Path` import; add `load_digest`; slim `today()`; add `digest_by_date`; add tests)
- Modify: `backend/src/api/mod.rs:112` (register `/news/digest/:date` route)

**Interfaces:**
- Consumes: `repo::articles`, `repo::quiz` (existing).
- Produces: `pub async fn digest_by_date(State<AppState>, Path<String>) -> Result<Json<TodayDto>, AppError>`; private `async fn load_digest(db: &Db, date: &str) -> Result<TodayDto, AppError>`. `TodayDto` shape unchanged.

- [ ] **Step 1: Write the failing tests**

Add to the `mod tests` block in `backend/src/api/news.rs` (it already has `state_with_db`, `today_wib`, `#[serial]`, and the `tower::ServiceExt` imports):

```rust
    #[serial]
    #[tokio::test]
    async fn digest_by_date_returns_stored_digest() {
        let state = state_with_db().await;
        crate::repo::news::insert(
            &state.db, "2026-06-18", "2026-06-18T00:00:00Z",
            &[crate::repo::news::NewArticle {
                position: 0, title: "Rust 2.0".into(), url: "https://ex.com/r".into(),
                source: "HN".into(), score: 10, summary: "rilis".into(),
                key_points_json: "[\"cepat\"]".into(), image_url: None, read_minutes: Some(4),
            }],
            &[crate::repo::news::NewQuiz {
                position: 0, article_pos: Some(0), question: "apa?".into(),
                options_json: "[\"x\",\"y\"]".into(), answer_index: 1, explanation: None,
            }],
        ).await.unwrap();

        let app = crate::api::router(state);
        let res = app.oneshot(
            Request::builder().uri("/news/digest/2026-06-18").body(Body::empty()).unwrap()
        ).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["available"], true);
        assert_eq!(v["date"], "2026-06-18");
        assert_eq!(v["articles"][0]["title"], "Rust 2.0");
        assert_eq!(v["quiz"][0]["answer_index"], 1);
    }

    #[serial]
    #[tokio::test]
    async fn digest_by_date_rejects_malformed_date() {
        let app = crate::api::router(state_with_db().await);
        let res = app.oneshot(
            Request::builder().uri("/news/digest/not-a-date").body(Body::empty()).unwrap()
        ).await.unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    }

    #[serial]
    #[tokio::test]
    async fn digest_by_date_unknown_date_is_unavailable() {
        let app = crate::api::router(state_with_db().await);
        let res = app.oneshot(
            Request::builder().uri("/news/digest/2020-01-01").body(Body::empty()).unwrap()
        ).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["available"], false);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd backend && cargo test digest_by_date`
Expected: FAIL to compile — route/handler `digest_by_date` does not exist (404 would also fail the asserts).

- [ ] **Step 3: Write the implementation**

In `backend/src/api/news.rs`, change the imports line:

```rust
use axum::{extract::State, Json};
```

to:

```rust
use axum::{extract::{Path, State}, Json};
```

Replace the existing `today()` function (lines 46-87) with the shared core plus two thin handlers:

```rust
/// Shared core: build the digest DTO for a WIB date string. Returns
/// available:false with empty vecs when no articles exist for that date.
async fn load_digest(db: &crate::db::Db, date: &str) -> Result<TodayDto, AppError> {
    let articles = repo::articles(db, date).await.map_err(AppError::Other)?;
    if articles.is_empty() {
        return Ok(TodayDto { available: false, date: None, articles: vec![], quiz: vec![] });
    }
    let quiz = repo::quiz(db, date).await.map_err(AppError::Other)?;
    Ok(TodayDto {
        available: true,
        date: Some(date.to_string()),
        articles: articles
            .into_iter()
            .map(|a| ArticleDto {
                position: a.position,
                title: a.title,
                url: a.url,
                source: a.source,
                summary: a.summary,
                key_points: decode_str_array(&a.key_points, "key_points"),
                image_url: a.image_url,
                read_minutes: a.read_minutes,
            })
            .collect(),
        quiz: quiz
            .into_iter()
            .map(|q| QuizDto {
                position: q.position,
                question: q.question,
                options: decode_str_array(&q.options, "options"),
                answer_index: q.answer_index,
                explanation: q.explanation,
                article_position: q.article_pos,
            })
            .collect(),
    })
}

/// Read-only: today's persisted digest. Never triggers generation.
pub async fn today(State(s): State<AppState>) -> Result<Json<TodayDto>, AppError> {
    let date = chrono::Utc::now()
        .with_timezone(&crate::assistant::time::wib())
        .format("%Y-%m-%d")
        .to_string();
    Ok(Json(load_digest(&s.db, &date).await?))
}

/// Read-only: the persisted digest for a specific WIB date (YYYY-MM-DD).
pub async fn digest_by_date(
    State(s): State<AppState>,
    Path(date): Path<String>,
) -> Result<Json<TodayDto>, AppError> {
    chrono::NaiveDate::parse_from_str(&date, "%Y-%m-%d")
        .map_err(|_| AppError::BadRequest(format!("invalid date: {date}")))?;
    Ok(Json(load_digest(&s.db, &date).await?))
}
```

In `backend/src/api/mod.rs`, after line 112 (`.route("/news/today", get(news::today))`) add:

```rust
        .route("/news/digest/:date", get(news::digest_by_date))
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd backend && cargo test digest_by_date && cargo test today_returns`
Expected: the `digest_by_date_*` tests PASS and the existing `today_returns_*` tests still PASS (refactor preserved behavior).

- [ ] **Step 5: Commit**

```bash
git add backend/src/api/news.rs backend/src/api/mod.rs
git commit -m "feat(news): GET /news/digest/:date — historical digest by date"
```

---

### Task 3: API — paginated dates endpoint

**Files:**
- Modify: `backend/src/api/news.rs` (add `Query` import; add `NewsDateDto`, `DatesQuery`, `dates` handler; add test)
- Modify: `backend/src/api/mod.rs` (register `/news/dates` route)

**Interfaces:**
- Consumes: `repo::news::dates` (Task 1).
- Produces: `pub async fn dates(State<AppState>, Query<DatesQuery>) -> Result<Json<Vec<NewsDateDto>>, AppError>` where `NewsDateDto { date: String, article_count: i64, created_at: String }`.

- [ ] **Step 1: Write the failing test**

Add to the `mod tests` block in `backend/src/api/news.rs`:

```rust
    #[serial]
    #[tokio::test]
    async fn dates_endpoint_lists_digests_newest_first() {
        let state = state_with_db().await;
        let art = crate::repo::news::NewArticle {
            position: 0, title: "t".into(), url: "https://ex.com/x".into(),
            source: "HN".into(), score: 1, summary: "s".into(),
            key_points_json: "[]".into(), image_url: None, read_minutes: None,
        };
        crate::repo::news::insert(&state.db, "2026-06-18", "2026-06-18T00:00:00Z", &[art], &[]).await.unwrap();

        let app = crate::api::router(state);
        let res = app.oneshot(
            Request::builder().uri("/news/dates").body(Body::empty()).unwrap()
        ).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v.as_array().unwrap().len(), 1);
        assert_eq!(v[0]["date"], "2026-06-18");
        assert_eq!(v[0]["article_count"], 1);
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd backend && cargo test dates_endpoint_lists_digests_newest_first`
Expected: FAIL — handler/route `dates` does not exist.

- [ ] **Step 3: Write the implementation**

In `backend/src/api/news.rs`, update the import to include `Query`:

```rust
use axum::{extract::{Path, Query, State}, Json};
```

Add near the other DTOs (after `TodayDto`, around line 35):

```rust
#[derive(Serialize)]
pub struct NewsDateDto {
    pub date: String,
    pub article_count: i64,
    pub created_at: String,
}

#[derive(serde::Deserialize)]
pub struct DatesQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}
```

Add the handler (after `digest_by_date`):

```rust
/// Read-only: digest dates, newest first, paginated. `limit` defaults to 30
/// (clamped 1..=100), `offset` defaults to 0.
pub async fn dates(
    State(s): State<AppState>,
    Query(q): Query<DatesQuery>,
) -> Result<Json<Vec<NewsDateDto>>, AppError> {
    let limit = q.limit.unwrap_or(30).clamp(1, 100);
    let offset = q.offset.unwrap_or(0).max(0);
    let rows = repo::dates(&s.db, limit, offset).await.map_err(AppError::Other)?;
    Ok(Json(
        rows.into_iter()
            .map(|r| NewsDateDto {
                date: r.digest_date,
                article_count: r.article_count,
                created_at: r.created_at,
            })
            .collect(),
    ))
}
```

In `backend/src/api/mod.rs`, after the `/news/digest/:date` route add:

```rust
        .route("/news/dates", get(news::dates))
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd backend && cargo test dates_endpoint_lists_digests_newest_first`
Expected: PASS.

- [ ] **Step 5: Run the full backend suite + commit**

Run: `cd backend && cargo test`
Expected: all tests PASS.

```bash
git add backend/src/api/news.rs backend/src/api/mod.rs
git commit -m "feat(news): GET /news/dates — paginated digest date list"
```

---

### Task 4: Frontend — schema + hooks

**Files:**
- Modify: `frontend/src/api/schemas.ts` (add `NewsDateSchema` + type after `NewsTodaySchema`)
- Modify: `frontend/src/api/hooks.ts` (import `NewsDateSchema`; add `useNewsDates`, `useNewsDigest`)
- Test: `frontend/src/api/news-hooks.test.ts` (new)

**Interfaces:**
- Produces: `NewsDateSchema` (zod), `useNewsDates(limit: number, offset?: number)`, `useNewsDigest(date: string | undefined)`.

- [ ] **Step 1: Write the failing test**

Create `frontend/src/api/news-hooks.test.ts`:

```ts
import { renderHook, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { http, HttpResponse } from "msw";
import React from "react";
import { server } from "../test/server";
import { useNewsDates, useNewsDigest } from "./hooks";

function wrapper() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return ({ children }: { children: React.ReactNode }) =>
    React.createElement(QueryClientProvider, { client: qc }, children);
}

test("useNewsDates fetches the date list", async () => {
  server.use(http.get("*/api/news/dates", () => HttpResponse.json(
    [{ date: "2026-06-18", article_count: 3, created_at: "2026-06-18T00:00:00Z" }],
  )));
  const { result } = renderHook(() => useNewsDates(30), { wrapper: wrapper() });
  await waitFor(() => expect(result.current.isSuccess).toBe(true));
  expect(result.current.data?.[0].date).toBe("2026-06-18");
  expect(result.current.data?.[0].article_count).toBe(3);
});

test("useNewsDigest is disabled when date is undefined", () => {
  const { result } = renderHook(() => useNewsDigest(undefined), { wrapper: wrapper() });
  expect(result.current.fetchStatus).toBe("idle");
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd frontend && npx vitest run src/api/news-hooks.test.ts`
Expected: FAIL — `useNewsDates`/`useNewsDigest` not exported.

- [ ] **Step 3: Write the implementation**

In `frontend/src/api/schemas.ts`, after the `NewsTodaySchema` block (and its exported types, around line 467) add:

```ts
export const NewsDateSchema = z.object({
  date: z.string(),
  article_count: z.number(),
  created_at: z.string(),
});

export type NewsDate = z.infer<typeof NewsDateSchema>;
```

In `frontend/src/api/hooks.ts`, add `NewsDateSchema` to the existing schema import block (the one that includes `NewsTodaySchema` around line 19), then add below the existing `useNewsToday` hook:

```ts
export const useNewsDates = (limit: number, offset = 0) =>
  useQuery({
    queryKey: ["news", "dates", limit, offset],
    queryFn: () => api.get(`/news/dates?limit=${limit}&offset=${offset}`, z.array(NewsDateSchema)),
    staleTime: 60 * 60 * 1000,
  });

export const useNewsDigest = (date: string | undefined) =>
  useQuery({
    queryKey: ["news", "digest", date],
    enabled: date != null,
    queryFn: () => api.get(`/news/digest/${date}`, NewsTodaySchema),
    staleTime: 60 * 60 * 1000,
  });
```

(`z` and `NewsTodaySchema` are already imported in `hooks.ts`.)

- [ ] **Step 4: Run test to verify it passes**

Run: `cd frontend && npx vitest run src/api/news-hooks.test.ts`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/api/schemas.ts frontend/src/api/hooks.ts frontend/src/api/news-hooks.test.ts
git commit -m "feat(news): NewsDate schema + useNewsDates/useNewsDigest hooks"
```

---

### Task 5: Frontend — extract shared NewsDigest component

**Files:**
- Create: `frontend/src/components/NewsDigest.tsx`
- Modify: `frontend/src/pages/NewsPage.tsx` (render the today branch via `NewsDigest`)
- Modify: `frontend/src/pages/NewsPage.test.tsx` (wrap render in `MemoryRouter`)

**Interfaces:**
- Produces: `NewsDigest({ date, articles, quiz }: { date: string; articles: NewsArticle[]; quiz: NewsQuizType[] })` default export.

- [ ] **Step 1: Create the component (refactor — existing NewsPage tests are the guard)**

Create `frontend/src/components/NewsDigest.tsx`:

```tsx
import NewsQuiz from "./NewsQuiz";
import type { NewsArticle, NewsQuiz as NewsQuizType } from "../api/schemas";

/** Renders one day's digest: header + article cards + retention quiz. */
export default function NewsDigest({
  date,
  articles,
  quiz,
}: {
  date: string;
  articles: NewsArticle[];
  quiz: NewsQuizType[];
}) {
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 24 }}>
      <header>
        <h1 style={{ fontSize: 20, fontWeight: 600, letterSpacing: "-0.015em" }}>Bacaan pagi</h1>
        <p style={{ fontSize: 13, color: "hsl(var(--muted-foreground))", marginTop: 2 }}>{date}</p>
      </header>
      {articles.map((a) => (
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
      <NewsQuiz questions={quiz} date={date} />
    </div>
  );
}
```

- [ ] **Step 2: Use it from NewsPage**

Replace the today branch in `frontend/src/pages/NewsPage.tsx`. The file becomes:

```tsx
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
```

- [ ] **Step 3: Run the existing NewsPage tests to verify the refactor is behavior-preserving**

Run: `cd frontend && npx vitest run src/pages/NewsPage.test.tsx`
Expected: both existing tests still PASS (no router needed yet — Task 6 adds the `Link`).

- [ ] **Step 4: Commit**

```bash
git add frontend/src/components/NewsDigest.tsx frontend/src/pages/NewsPage.tsx
git commit -m "refactor(news): extract shared NewsDigest component"
```

---

### Task 6: Frontend — Arsip (archive) list on NewsPage

**Files:**
- Modify: `frontend/src/pages/NewsPage.tsx` (add archive section + load-more)
- Modify: `frontend/src/pages/NewsPage.test.tsx` (wrap renders in `MemoryRouter`; add `/news/dates` mock + archive test)

**Interfaces:**
- Consumes: `useNewsDates` (Task 4), `NewsDigest` (Task 5), `Link` from `react-router-dom`.

- [ ] **Step 1: Write the failing test**

Update `frontend/src/pages/NewsPage.test.tsx`. Change imports and `renderPage` to wrap in a router, give every test a default `/news/dates` handler, and add the archive test:

```tsx
import { render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";
import { http, HttpResponse } from "msw";
import { server } from "../test/server";
import NewsPage from "./NewsPage";

function renderPage() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter><NewsPage /></MemoryRouter>
    </QueryClientProvider>,
  );
}

test("renders articles and key points", async () => {
  server.use(http.get("*/api/news/dates", () => HttpResponse.json([])));
  server.use(http.get("*/api/news/today", () => HttpResponse.json({
    available: true, date: "2026-06-16",
    articles: [{ position: 0, title: "Rust 2.0", url: "https://ex.com/r", source: "HN", summary: "rilis besar", key_points: ["lebih cepat"], image_url: "https://ex.com/i.png", read_minutes: 4 }],
    quiz: [],
  })));
  renderPage();
  expect(await screen.findByText("Rust 2.0")).toBeInTheDocument();
  expect(await screen.findByText("lebih cepat")).toBeInTheDocument();
  expect(await screen.findByText(/4 mnt/)).toBeInTheDocument();
});

test("shows an empty state when no digest yet", async () => {
  server.use(http.get("*/api/news/dates", () => HttpResponse.json([])));
  server.use(http.get("*/api/news/today", () => HttpResponse.json({ available: false, date: null, articles: [], quiz: [] })));
  renderPage();
  expect(await screen.findByText(/belum siap/i)).toBeInTheDocument();
});

test("lists archive dates linking to the detail route", async () => {
  server.use(http.get("*/api/news/today", () => HttpResponse.json({ available: false, date: null, articles: [], quiz: [] })));
  server.use(http.get("*/api/news/dates", () => HttpResponse.json([
    { date: "2026-06-18", article_count: 3, created_at: "2026-06-18T00:00:00Z" },
  ])));
  renderPage();
  const link = await screen.findByRole("link", { name: /3 artikel/i });
  expect(link).toHaveAttribute("href", "/news/2026-06-18");
});
```

- [ ] **Step 2: Run test to verify the new test fails**

Run: `cd frontend && npx vitest run src/pages/NewsPage.test.tsx`
Expected: the "lists archive dates" test FAILS (no archive section yet); the other two still pass.

- [ ] **Step 3: Implement the archive section**

Update `frontend/src/pages/NewsPage.tsx`:

```tsx
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
```

Note: the accessible name of each archive link includes both the formatted date and "{n} artikel", so the test's `/3 artikel/i` name matcher resolves the link.

- [ ] **Step 4: Run test to verify it passes**

Run: `cd frontend && npx vitest run src/pages/NewsPage.test.tsx`
Expected: all three tests PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/pages/NewsPage.tsx frontend/src/pages/NewsPage.test.tsx
git commit -m "feat(news): Arsip date list on /news with load-more"
```

---

### Task 7: Frontend — NewsDatePage detail route

**Files:**
- Create: `frontend/src/pages/NewsDatePage.tsx`
- Create: `frontend/src/pages/NewsDatePage.test.tsx`
- Modify: `frontend/src/App.tsx` (import + `news/:date` route)

**Interfaces:**
- Consumes: `useNewsDigest` (Task 4), `NewsDigest` (Task 5), `useParams`/`Link` from `react-router-dom`.

- [ ] **Step 1: Write the failing test**

Create `frontend/src/pages/NewsDatePage.test.tsx`:

```tsx
import { render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { http, HttpResponse } from "msw";
import { server } from "../test/server";
import NewsDatePage from "./NewsDatePage";

function renderAt(date: string) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter initialEntries={[`/news/${date}`]}>
        <Routes><Route path="news/:date" element={<NewsDatePage />} /></Routes>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

test("renders a stored historical digest", async () => {
  server.use(http.get("*/api/news/digest/2026-06-18", () => HttpResponse.json({
    available: true, date: "2026-06-18",
    articles: [{ position: 0, title: "Old News", url: "https://ex.com/o", source: "HN", summary: "ringkas", key_points: [], image_url: null, read_minutes: null }],
    quiz: [],
  })));
  renderAt("2026-06-18");
  expect(await screen.findByText("Old News")).toBeInTheDocument();
});

test("shows empty state when that date has no digest", async () => {
  server.use(http.get("*/api/news/digest/2020-01-01", () => HttpResponse.json({ available: false, date: null, articles: [], quiz: [] })));
  renderAt("2020-01-01");
  expect(await screen.findByText(/tidak ada digest/i)).toBeInTheDocument();
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd frontend && npx vitest run src/pages/NewsDatePage.test.tsx`
Expected: FAIL — module `./NewsDatePage` not found.

- [ ] **Step 3: Create the page**

Create `frontend/src/pages/NewsDatePage.tsx`:

```tsx
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
```

- [ ] **Step 4: Register the route**

In `frontend/src/App.tsx`, add the import alongside the other page imports, and add the route immediately after the existing `news` route (line 42):

```tsx
        <Route path="news/:date" element={<NewsDatePage />} />
```

Add near the top with the other page imports:

```tsx
import NewsDatePage from "./pages/NewsDatePage";
```

- [ ] **Step 5: Run tests + commit**

Run: `cd frontend && npx vitest run src/pages/NewsDatePage.test.tsx`
Expected: both tests PASS.

```bash
git add frontend/src/pages/NewsDatePage.tsx frontend/src/pages/NewsDatePage.test.tsx frontend/src/App.tsx
git commit -m "feat(news): /news/:date detail page for historical digests"
```

---

### Task 8: Full-suite verification

**Files:** none (verification only)

- [ ] **Step 1: Backend suite**

Run: `cd backend && cargo test`
Expected: all PASS.

- [ ] **Step 2: Frontend suite + typecheck/build**

Run: `cd frontend && npm test && npm run build`
Expected: all tests PASS and the production build (incl. TypeScript typecheck) succeeds.

- [ ] **Step 3: Commit any incidental fixes** (only if Steps 1–2 surfaced something)

```bash
git add -A && git commit -m "test(news): fix incidental issues found in full-suite run"
```

---

## Self-Review Notes

- **Spec coverage:** `/news/dates` → Task 3; `/news/digest/:date` → Task 2; `load_digest` refactor → Task 2; `dates()` repo → Task 1; shared `NewsDigest` → Task 5; Arsip list + load-more → Task 6; `NewsDatePage` + route → Task 7; schemas/hooks → Task 4; error handling (400 bad date, 200 unavailable) → Task 2; testing → every task + Task 8. All spec sections covered.
- **Type consistency:** `DateRow{digest_date,created_at,article_count}` (Task 1) → mapped to `NewsDateDto{date,article_count,created_at}` (Task 3) → `NewsDateSchema{date,article_count,created_at}` (Task 4). `load_digest`/`digest_by_date`/`dates` signatures consistent across backend tasks. `NewsDigest({date,articles,quiz})` props consistent across Tasks 5–7. `TodayDto` reused unchanged for the detail endpoint and `NewsTodaySchema` for its client decode.
- **Known minor:** today's digest also appears in the Arsip list once generated (it's a digest date too) — accepted as harmless redundancy per the design's YAGNI scope.
