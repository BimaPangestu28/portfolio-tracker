# Morning IT News in the Briefing — Design

**Date:** 2026-06-16
**Status:** Approved (pending spec review)
**Owner:** Bima

## Goal

Add a daily "Bacaan pagi" (morning reading) section to the existing Telegram
morning briefing: a short, curated list of the latest trending IT/dev articles,
tailored to the owner's stack (Rust, blockchain/web3, AI/LLM, cloud, TypeScript),
so he stays up to date each morning.

## Decisions (locked)

- **Sources:** Hacker News (Algolia front-page API) + a configurable list of RSS/Atom feeds.
- **Delivery:** a new section folded into the existing morning briefing (one Telegram
  message, default 07:00 WIB). No separate send, no web dashboard card.
- **Focus:** tailored to the owner's stack via deterministic keyword scoring; the LLM
  picks the final 3 from a pre-scored shortlist.
- **Architecture:** Approach A — deterministic gather + LLM curation inside the existing
  briefing `compose` call (a single LLM call for the whole briefing).

## Non-goals (YAGNI)

- No separate news send or schedule.
- No web dashboard card.
- No per-user feed management UI; feeds are configured via env.
- No full-text article fetch/summarization; we work from titles + metadata only.

## Architecture

Follows the established proactive pattern: deterministic gather → LLM compose
(with fallback) → Telegram. News is one more independently-degrading source inside
the briefing's `gather`, exactly like the existing ClickUp and Gmail sections.

```
briefing::gather(db)
  ├── (existing: todos, reminders, events, portfolio, movers, clickup, gmail, memory)
  └── news::shortlist(db) -> Vec<Article>     // new; degrades to empty on any error
        ├── hackernews::fetch()   -> Vec<Article>
        ├── rss::fetch(feeds)     -> Vec<Article>
        ├── merge + dedup-by-url
        ├── keyword pre-score (relevance)
        ├── drop recently-seen (news_seen table)
        └── sort (relevance desc, score desc), take <= NEWS_MAX_CANDIDATES

briefing::render_data_block(data)
  └── append "Kandidat bacaan IT (pilih maks 3 paling relevan):" block

compose(BRIEFING_SYSTEM, block, ...)          // extended prompt selects final 3

briefing::run -> on successful send_message -> news::seen::mark(db, shortlist_urls)
```

### Module layout

New module `backend/src/assistant/proactive/news/`:

- `mod.rs` — `Article` type, `shortlist(db) -> Vec<Article>`, keyword scoring,
  merge/dedup. Re-exports the submodules.
- `hackernews.rs` — fetch + parse the HN Algolia response.
- `rss.rs` — fetch + parse RSS/Atom feeds.
- `seen.rs` — `mark(db, &[url])`, `filter_unseen(db, candidates)`, `prune(db)` against
  the `news_seen` table.

### Types

```rust
pub struct Article {
    pub title: String,
    pub url: String,
    pub source: String,          // "HN", "The Verge", ...
    pub score: i64,              // HN points; RSS = 0
    pub published_at: Option<String>,
    pub relevance: i32,          // keyword pre-score (count of matched stack keywords)
}
```

## Data flow details

### Sources

- **Hacker News:** `https://hn.algolia.com/api/v1/search?tags=front_page` (JSON, no API
  key). Keep only hits with a non-empty `url` (skip Ask HN / text posts). Map
  `points -> score`, `created_at -> published_at`, `title`, source `"HN"`. Optional
  minimum-points floor to cut noise.
- **RSS/Atom:** feed URLs from `NEWS_RSS_FEEDS` (comma-separated env), with a tailored
  default set (e.g. InfoQ, The New Stack, r/rust, r/programming). Parsed with the
  `feed-rs` crate (handles both RSS and Atom). `score = 0` for RSS; `source` = feed title
  or host.

Each source is fetched independently through a `reqwest` client with a request timeout.
A failing source logs `tracing::warn!` and contributes nothing. If **all** sources fail,
the candidate list is empty and the section is omitted — the briefing still sends. This
matches the existing degrade behaviour of the ClickUp/Gmail sections.

### Keyword scoring (tailored, deterministic)

A `const` keyword set grouped by stack interest:

- `rust`
- `blockchain`, `web3`, `solidity`, `ethereum`
- `ai`, `llm`, `agent`, `model`
- `cloud`, `azure`, `aws`, `kubernetes`, `databricks`
- `typescript`, `react`

`relevance` = number of distinct keywords matched in the (lowercased) title. Candidates
are sorted by `(relevance desc, score desc)`, recently-seen ones removed, then truncated
to `NEWS_MAX_CANDIDATES` (default 12). Articles with `relevance == 0` are dropped so the
shortlist stays on-topic.

### Recently-seen dedup

Migration `backend/migrations/0022_news_seen.sql`:

```sql
CREATE TABLE news_seen (
    url_hash   TEXT PRIMARY KEY,   -- stable hash of the normalized url
    url        TEXT NOT NULL,
    first_seen TEXT NOT NULL       -- RFC3339 UTC
);
```

- `filter_unseen` removes candidates whose `url_hash` already exists.
- `mark` inserts the **entire shortlist** that was handed to the LLM — called **only after
  the briefing is sent successfully** (`briefing::run`, after `send_message` returns Ok).
  If the send fails, nothing is marked and the articles remain eligible the next day.
- `prune` deletes rows with `first_seen` older than 14 days.

**Accepted trade-off:** a shortlisted article the LLM did *not* pick is still suppressed
the following day. This keeps the seen-set logic simple (code never needs to parse which
3 the LLM chose). Documented here intentionally.

### Briefing integration

- `BriefingData` gains `news: Vec<Article>` (empty on any gather error → independent degrade).
- `render_data_block` appends, when `news` is non-empty:

  ```
  Kandidat bacaan IT (pilih maks 3 paling relevan):
  - <judul> | <sumber> | <skor> | <url>
  ...
  ```

- `BRIEFING_SYSTEM` is extended with one instruction block: render a short "Bacaan pagi"
  section selecting **at most 3** candidates most relevant to the owner's stack
  (Rust / blockchain / AI / cloud / TS), each as one Bahasa Indonesia line explaining why
  it is relevant, **copying the URL exactly as written**, and skipping the section entirely
  when there are no candidates. The existing "copy numbers exactly, never invent" rule
  already guards against fabricated content; "salin URL persis" extends it to links.

### Known risk

In Approach A the LLM re-types the URL, so a malformed link is possible. Mitigated by the
explicit "salin URL persis" instruction and by keeping URLs short. If this proves unreliable
in practice, the fallback hardening is: have the code emit the final `judul — url` lines
deterministically and let the LLM write only the one-line summary. Out of scope for v1.

## Configuration

Documented in `backend/.env.example`:

- `NEWS_ENABLED` — default `true`; `off` disables the section.
- `NEWS_RSS_FEEDS` — comma-separated feed URLs; default tailored set.
- `NEWS_MAX_CANDIDATES` — default 12.

## Error handling

- Per-source failures: `tracing::warn!` + skip; never propagate out of `shortlist`.
- All-sources-fail / `NEWS_ENABLED=off`: empty candidates → section omitted → briefing unaffected.
- `mark_seen` failure after send: logged, non-fatal (worst case an article repeats once).
- No `unwrap()`/`panic!()` on any network, parse, or DB path.

## Testing

All tests run without network — HTTP responses are parsed from fixture strings.

- `hackernews`: parse a sample Algolia JSON fixture → expected `Article`s; Ask HN entries
  (no url) are dropped.
- `rss`: parse a sample RSS and a sample Atom fixture → expected `Article`s.
- scoring: keyword matches produce the expected `relevance`; `relevance == 0` dropped;
  sort order is `(relevance desc, score desc)`.
- merge/dedup: duplicate URLs across sources collapse to one.
- `render_data_block`: includes the "Kandidat bacaan IT" block when news present, omits it
  when empty.
- `BRIEFING_SYSTEM`: contains the news/"Bacaan pagi" instruction (mirrors the existing
  prompt-content tests).
- `seen`: `mark` then `filter_unseen` suppresses marked URLs; `prune` drops old rows.
  Run against `sqlite::memory:`.

## New dependencies

- `feed-rs` — RSS/Atom parsing (no existing XML parser in the backend).

## Migrations

- `backend/migrations/0022_news_seen.sql` — the `news_seen` table.
