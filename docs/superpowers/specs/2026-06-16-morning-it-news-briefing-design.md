# Morning IT News: Briefing Section + Web Digest Page — Design

**Date:** 2026-06-16
**Status:** Approved (pending spec review)
**Owner:** Bima

## Goal

Keep the owner up to date on trending IT/dev news, tailored to his stack
(Rust, blockchain/web3, AI/LLM, cloud, TypeScript), via two surfaces fed by one
**daily news digest**:

1. A "Bacaan pagi" section in the existing Telegram morning briefing — the day's
   top 3 articles as one-line links.
2. A new **web page** (`/news`) with each article's **summary + key points**, plus a
   **retention quiz** generated from that day's articles.

## Decisions (locked)

- **Sources:** Hacker News (Algolia front-page API) + a configurable list of RSS/Atom feeds.
- **Selection:** deterministic keyword pre-scoring; **code** picks the final 3 (not the LLM),
  so the set is stable, persisted, and shared by both surfaces. URLs are owned by code.
- **Summary depth:** fetch each chosen article's page, extract the main text, and have the
  LLM produce a summary + key points. Degrades to title+RSS-snippet on fetch/extract failure.
- **Quiz:** generated from the day's chosen articles to test retention (multiple-choice).
- **Generation timing:** a scheduled morning job generates and **persists** the digest once
  per WIB day, before the briefing. Both the briefing and the web page read the persisted row.
- **Web page scope:** the same top 3 the briefing links to, shown in full (summary + key
  points + quiz).

## Non-goals (YAGNI)

- No per-user feed-management UI; feeds are env-configured.
- No quiz-attempt history/scoring persistence in v1 (the quiz is interactive, client-scored).
- No news archive/history page in v1 (`GET /news/today` only; history can come later).
- No separate news Telegram send — it rides inside the existing briefing.

## Architecture overview

One daily digest, generated once and persisted, consumed by two read paths:

```
proactive tick (every 5 min)
  └── news_digest_due (NEWS_DIGEST_HOUR_WIB, default 06) -> news::digest::ensure_today(db)

news::digest::ensure_today(db) -> Digest        // idempotent per WIB date; read-or-generate
  ├── return persisted row if today's digest exists
  └── else generate():
        1. news::shortlist(db)        -> candidates (HN + RSS, keyword-scored, unseen)
        2. pick top 3                 (relevance desc, score desc)  // CODE picks
        3. for each: fetch -> extract main text -> LLM {summary, key_points[]}
                                      (degrade to title+snippet on failure)
        4. LLM quiz from the 3 summaries -> [{question, options[], answer_index, explanation}]
        5. persist (digest + articles + quiz) in one transaction
        6. news::seen::mark(candidate urls)

briefing::gather(db)
  └── news::digest::ensure_today(db)  -> top 3 -> deterministic "Bacaan pagi" lines

GET /api/news/today  (JWT)
  └── repo::news::today(db)           -> { articles[], quiz[] }   // read-only

frontend /news (NewsPage)
  └── useNewsToday() -> render articles (summary + key points) + interactive quiz
```

`ensure_today` is the single generation path. The morning job calls it so the digest is
ready before the briefing; `briefing::gather` also calls it (normally a cheap read; a safety
net if the job did not run). The web endpoint is **read-only** — it never triggers the heavy
generation on a request; if today's digest is not ready it returns `{ available: false }`.

### Concurrency

`ensure_today` must not double-generate when the job and the briefing race. It claims a
dedup key via the existing `proactive_log::try_claim` (`news_digest:YYYY-MM-DD`); the loser
re-reads the persisted row. The `news_digest` table's `digest_date` primary key is the final
guard (insert-or-ignore).

## Backend

### Module layout

`backend/src/assistant/proactive/news/`:

- `mod.rs` — `Article` candidate type, `shortlist(db) -> Vec<Article>`, keyword scoring, merge/dedup.
- `hackernews.rs` — fetch + parse the HN Algolia response.
- `rss.rs` — fetch + parse RSS/Atom feeds (`feed-rs`).
- `extract.rs` — fetch an article URL and extract main text (size-capped, timed out).
- `digest.rs` — `ensure_today`, `generate`, LLM summary + quiz steps, persistence orchestration.
- `seen.rs` — `mark`, `filter_unseen`, `prune` against `news_seen`.

`backend/src/repo/news.rs` — all digest SQL (`today`, `insert_digest`, etc.).
`backend/src/api/news.rs` — `GET /news/today` handler.

### Candidate gather (unchanged from the first design)

- **HN:** `https://hn.algolia.com/api/v1/search?tags=front_page` (JSON, no key). Keep hits
  with a non-empty `url`; map `points -> score`, `created_at -> published_at`, source `"HN"`.
- **RSS/Atom:** `NEWS_RSS_FEEDS` (comma-separated; tailored default set). `score = 0`.
- Each source fetched independently with a timeout; failures `warn!` + skip. All-fail → empty.
- **Keyword score:** `const` set grouped — `rust` · `blockchain/web3/solidity/ethereum` ·
  `ai/llm/agent/model` · `cloud/azure/aws/kubernetes/databricks` · `typescript/react`.
  `relevance` = distinct keywords matched in the lowercased title. Drop `relevance == 0`.
  Sort `(relevance desc, score desc)`, remove recently-seen, take ≤ `NEWS_MAX_CANDIDATES` (12).

### Article fetch + extract

- `extract::fetch_main_text(url) -> Option<String>`: GET with a `reqwest` client (timeout,
  redirect cap), reject non-`http(s)` URLs and non-HTML / oversized bodies (response size cap),
  then extract the main article text. Extraction crate: **`readability`** (Mozilla-style main
  content); if it proves unsuitable in implementation, fall back to `html2text` over the body.
- On any failure (paywall, JS-only, timeout, parse): return `None`. The summary step then
  degrades to the title + RSS snippet so the article still appears.

### LLM steps

Reuse the existing `llm::claude::ClaudeClient` (Anthropic-Messages shape; DeepSeek by default).
Two prompt families, both with deterministic fallbacks (consistent with `compose.rs`):

- **Summary:** input = title + source + extracted text (or snippet). Output JSON
  `{ "summary": string, "key_points": string[] }`. Parsed strictly; on LLM failure or
  unparseable output, fall back to `{ summary: snippet-or-title, key_points: [] }`.
- **Quiz:** input = the 3 articles' summaries + key points. Output JSON array of
  `{ "question", "options": string[], "answer_index": int, "explanation": string,
  "article_position": int }`, `NEWS_QUIZ_COUNT` items (default 4). On failure, the digest is
  still persisted **without** a quiz (the page shows articles only).

JSON is requested in the system prompt and parsed with `serde_json`; a `tracing::warn!` plus
fallback covers any deviation. No `unwrap()` on LLM output.

### Persistence — migration `0022_news_digest.sql`

```sql
CREATE TABLE news_digest (
    digest_date TEXT PRIMARY KEY,                 -- WIB YYYY-MM-DD
    created_at  TEXT NOT NULL                     -- RFC3339 UTC
);

CREATE TABLE news_article (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    digest_date TEXT NOT NULL REFERENCES news_digest(digest_date) ON DELETE CASCADE,
    position    INTEGER NOT NULL,                 -- 0..2
    title       TEXT NOT NULL,
    url         TEXT NOT NULL,
    source      TEXT NOT NULL,
    score       INTEGER NOT NULL DEFAULT 0,
    summary     TEXT NOT NULL,
    key_points  TEXT NOT NULL                     -- JSON array of strings
);

CREATE TABLE news_quiz_question (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    digest_date  TEXT NOT NULL REFERENCES news_digest(digest_date) ON DELETE CASCADE,
    position     INTEGER NOT NULL,
    article_pos  INTEGER,                         -- which article it tests (nullable)
    question     TEXT NOT NULL,
    options      TEXT NOT NULL,                   -- JSON array of strings
    answer_index INTEGER NOT NULL,
    explanation  TEXT
);

CREATE TABLE news_seen (
    url_hash   TEXT PRIMARY KEY,                  -- stable hash of the normalized url
    url        TEXT NOT NULL,
    first_seen TEXT NOT NULL                      -- RFC3339 UTC
);
```

Digest + articles + quiz are written in one transaction. `news_seen` retains 14 days
(`prune` on each generation). Foreign keys are ON (see `db::connect`).

### Briefing integration

- `BriefingData` gains `news: Vec<DigestArticle>` (top 3 from `ensure_today`; empty on error
  → independent degrade, matching ClickUp/Gmail).
- `render_data_block` appends, when non-empty, a finished block the LLM passes through verbatim:

  ```
  Bacaan pagi (sertakan apa adanya, jangan ubah link):
  - <judul> — <ringkasan 1 baris> <url>
  ```

- `BRIEFING_SYSTEM` gains one line: include the "Bacaan pagi" lines exactly as given (links
  unchanged), skip when absent. Because code supplies the final lines and URLs, there is **no**
  URL-hallucination risk (the open issue from the first design is resolved).

### API

- `GET /api/news/today` (JWT-protected group in `api::router`):
  `{ available: bool, date: string|null, articles: [...], quiz: [...] }`.
  `articles[]`: `{ position, title, url, source, summary, key_points: string[] }`.
  `quiz[]`: `{ position, question, options: string[], answer_index, explanation, article_position }`.
  Shipping `answer_index` to the client is intentional — the quiz is a personal retention
  check, scored client-side, not graded.

### Config (documented in `backend/.env.example`)

- `NEWS_ENABLED` — default `true`; `off` disables generation and both surfaces.
- `NEWS_DIGEST_HOUR_WIB` — default `6` (before the 7am briefing); `off` disables the job.
- `NEWS_RSS_FEEDS` — comma-separated feed URLs; tailored default set.
- `NEWS_MAX_CANDIDATES` — default `12`.
- `NEWS_QUIZ_COUNT` — default `4`.

## Frontend

- **Route + nav:** add `news` to `App.tsx` and a nav item to `AppShell`.
- **API layer:** Zod schemas (`newsDigestSchema`, articles, quiz) in `api/schemas.ts`;
  `useNewsToday()` query hook in `api/hooks.ts`. All responses validated before use.
- **`pages/NewsPage.tsx`:**
  - Header + date. If `available === false`: empty state ("Digest hari ini belum siap").
  - Articles: title (link, opens in new tab), source badge, summary paragraph, key-points
    bullet list.
  - Quiz (`components/NewsQuiz.tsx`): renders questions with radio options; on submit reveals
    correct/incorrect per question, the explanation, and a total score. Client-side scoring
    against `answer_index`; no network submit. Reset-to-retry allowed.
- Bahasa Indonesia copy, Tailwind + Radix, matching existing page/card patterns.

## Error handling

- Per-source / per-article failures: `warn!` + degrade; never abort the digest.
- LLM summary/quiz failure: fall back (snippet summary; quiz omitted). Digest still persists.
- Generation failure as a whole: `warn!`; the briefing omits the section; the web page shows
  the empty state. No user-visible crash, no `unwrap()`/`panic!()` on net/parse/DB paths.
- `fetch_main_text` guards: http(s) only, timeout, redirect cap, response-size cap.

## Testing

Backend — all without network (HTTP/HTML/LLM outputs from fixture strings):

- `hackernews`: parse Algolia JSON fixture; url-less (Ask HN) entries dropped.
- `rss`: parse RSS and Atom fixtures.
- scoring: relevance counts, `relevance == 0` dropped, sort order.
- merge/dedup: duplicate URLs across sources collapse.
- `extract`: main-text extraction on an HTML fixture; non-html/oversized rejected.
- summary/quiz JSON parsing: valid fixture parses; malformed → fallback.
- `repo::news` + persistence: insert a digest, read it back via `today`; cascade on delete;
  `seen` mark/filter/prune. Against `sqlite::memory:`.
- `render_data_block`: contains the "Bacaan pagi" block when present, omitted when empty.
- `BRIEFING_SYSTEM`: contains the pass-through-links instruction.

Frontend (vitest + testing-library, MSW for the endpoint):

- `useNewsToday` parses a mocked response; invalid shape rejected.
- `NewsPage`: renders articles + key points; empty state when `available === false`.
- `NewsQuiz`: selecting answers + submit scores correctly and reveals explanations.

## New dependencies

- `feed-rs` — RSS/Atom parsing.
- `readability` — HTML main-text extraction (fallback `html2text` if unsuitable).

## Migrations

- `backend/migrations/0022_news_digest.sql` — `news_digest`, `news_article`,
  `news_quiz_question`, `news_seen`.

## Suggested implementation phases (for the plan)

1. **Backend digest core:** sources + scoring + seen + extract + LLM summary/quiz + persistence
   + migration. Unit-tested in isolation.
2. **Briefing integration:** `ensure_today`, proactive `news_digest` job, "Bacaan pagi" block.
3. **API + frontend:** `GET /news/today`, schemas/hook, `NewsPage` + `NewsQuiz`, route + nav.
