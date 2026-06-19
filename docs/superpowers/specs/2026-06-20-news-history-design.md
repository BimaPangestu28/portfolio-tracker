# News History — Design Spec

**Date:** 2026-06-20
**Status:** Approved (pending implementation plan)
**Branch target:** new feature branch off `main`

## Problem

The news digest feature scrapes and stores a digest every morning (06:00–11:00 WIB window) into the `news_digest` / `news_article` / `news_quiz_question` tables, which persist on the SQLite volume. However, the only way to view a digest is `GET /news/today`, which queries strictly the current WIB date. The frontend `/news` page calls only that endpoint.

Consequently, **past digests are stored but unreachable** — there is no API endpoint or UI to browse history. To a user this reads as "there is no history of scraped news." (Separately, before 06:00 WIB the current day's digest legitimately does not exist yet; that message — "Digest berita hari ini belum siap. Cek lagi nanti pagi ya." — is correct behavior and is out of scope here.)

## Goal

Let the user browse previously scraped digests: a list of dates on the existing `/news` page, each opening a detail view identical to the today view (articles + quiz).

## Non-Goals (YAGNI)

- No automatic deletion / retention policy — keep all digests.
- No search or filtering of articles.
- No manual "regenerate digest" trigger.
- No changes to the scraping / generation pipeline.

## Decisions (from brainstorming)

| Question | Decision |
|----------|----------|
| Browse model | Date list on the same `/news` page → click a date to open its detail. |
| Retention shown | All digests, with pagination (load-more) when the list grows. |
| Detail content | Articles **and** quiz — identical to the today view, reusing the same component. |
| API shape | Two new endpoints with static path segments; refactor `today()` to share a core. |

## Architecture

Approach A: two new read-only endpoints reusing the existing repo functions, plus a shared presentational component on the frontend.

### Backend — repo layer (`backend/src/repo/news.rs`)

Add a row type and a paginated lister:

```rust
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct DateRow {
    pub digest_date: String,
    pub created_at: String,
    pub article_count: i64,
}

/// Distinct digest dates, newest first, with article counts. Paginated.
pub async fn dates(db: &Db, limit: i64, offset: i64) -> anyhow::Result<Vec<DateRow>>;
```

Query:

```sql
SELECT d.digest_date, d.created_at, COUNT(a.position) AS article_count
FROM news_digest d
LEFT JOIN news_article a ON a.digest_date = d.digest_date
GROUP BY d.digest_date, d.created_at
ORDER BY d.digest_date DESC
LIMIT ? OFFSET ?
```

The existing `articles(db, date)` and `quiz(db, date)` already take a date parameter and are reused unchanged for the detail view.

### Backend — API layer (`backend/src/api/news.rs`, routes in `backend/src/api/mod.rs`)

- Extract the body of `today()` into a shared core `load_digest(db: &Db, date: &str) -> Result<TodayDto, AppError>` that returns `available: false` with empty vectors when no articles exist for the date (current behavior). `today()` becomes: compute the WIB date, then delegate to `load_digest`. **The `TodayDto` response shape is unchanged.**
- `GET /news/dates` → `Json<Vec<NewsDateDto>>` where `NewsDateDto { date: String, article_count: i64, created_at: String }`. Query params via a `Pagination { limit: Option<i64>, offset: Option<i64> }` struct: `limit` defaults to 30 and is clamped to `[1, 100]`; `offset` defaults to 0 and is clamped to `>= 0`.
- `GET /news/digest/:date` → `Json<TodayDto>`. Validate the `:date` path segment by parsing with `chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")`; on parse error return **400** (`AppError`). On success delegate to `load_digest`. A valid-but-absent date returns **200** with `available: false`.

Routes (static segments precede the param route; axum/matchit prioritizes static matches, so no ambiguity with `/news/today`):

```rust
.route("/news/today", get(news::today))
.route("/news/dates", get(news::dates))
.route("/news/digest/:date", get(news::digest_by_date))
```

### Frontend (`frontend/src`)

- **Extract** a presentational component `NewsDigest` from the current `NewsPage` body — it takes `{ date, articles, quiz }` and renders the header, the article cards, and `<NewsQuiz>`. Used by both the today page and the date detail page so the detail view is identical to today.
- **`NewsPage`** (`/news`): unchanged today behavior (digest or "belum siap" message), then below it an **"Arsip"** section listing dates from `/news/dates`. Each row is a `react-router` `Link` to `/news/:date` showing the formatted date and article count (e.g. "19 Jun 2026 · 3 artikel"). A **"Muat lebih banyak"** button increases the fetched page (limit/offset) when more dates exist.
- **`NewsDatePage`** (new, route `news/:date` added in `App.tsx`): reads `:date` param, fetches `/news/digest/:date`, renders `<NewsDigest>`, with a "← Kembali" link to `/news`. When `available: false`, shows "Tidak ada digest untuk tanggal ini."
- **Schemas** (`api/schemas.ts`): add `NewsDateSchema = z.object({ date, article_count, created_at })`; reuse `NewsTodaySchema` for the detail response.
- **Hooks** (`api/hooks.ts`): `useNewsDates(limit, offset)` and `useNewsDigest(date)` (the latter `enabled` only when `date` is present). Reuse the same long `staleTime` rationale (digests are immutable once written, so cache aggressively).

## Data Flow

```
/news page
  ├─ GET /news/today          → today's digest (or available:false)
  └─ GET /news/dates?limit&offset → [{date, article_count, created_at}] (newest first)
        └─ click a date → navigate /news/:date
              └─ GET /news/digest/:date → TodayDto → <NewsDigest>
```

## Error Handling

| Case | Behavior |
|------|----------|
| Invalid date format in `/news/digest/:date` | 400 (AppError) |
| Valid date with no stored digest | 200, `available: false` |
| DB error (any endpoint) | 500 via `AppError::Other` |
| `limit`/`offset` out of range | Clamped to valid bounds (no error) |

## Testing

**Backend (`cargo test --lib`):**
- `repo::news::dates` — newest-first ordering, correct `article_count`, pagination (`limit`/`offset`) across multiple inserted digests.
- API `/news/dates` — returns the list in expected shape and order.
- API `/news/digest/:date` — returns the stored digest for a present date (articles + quiz), `available: false` for an absent date, and **400** for a malformed date string.
- Existing `today()` tests must continue to pass after the `load_digest` refactor.

**Frontend (vitest):**
- `NewsPage` renders the Arsip section from a mocked `/news/dates` and links to the date route.
- `NewsDatePage` renders a digest from a mocked `/news/digest/:date`, and renders the "Tidak ada digest" state when `available: false`.

## Files Touched

- `backend/src/repo/news.rs` — add `DateRow` + `dates()`.
- `backend/src/api/news.rs` — add `load_digest`, `dates`, `digest_by_date`, `NewsDateDto`, `Pagination`; refactor `today()`.
- `backend/src/api/mod.rs` — register two routes.
- `frontend/src/components/NewsDigest.tsx` — new shared component.
- `frontend/src/pages/NewsPage.tsx` — render today via `NewsDigest` + Arsip section.
- `frontend/src/pages/NewsDatePage.tsx` — new detail page.
- `frontend/src/api/schemas.ts` — `NewsDateSchema`.
- `frontend/src/api/hooks.ts` — `useNewsDates`, `useNewsDigest`.
- `frontend/src/App.tsx` — `news/:date` route.
- Test files alongside the above.
