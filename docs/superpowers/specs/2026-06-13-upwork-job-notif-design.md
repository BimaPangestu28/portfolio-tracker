# Upwork Job & Invitation Notifications — Design

**Date:** 2026-06-13
**Status:** Approved (design); pending implementation plan
**Scope:** Sub-project 2 of the Upwork integration. Poll Upwork for new direct invitations and
marketplace jobs matching the owner's skills, and push the relevant ones to Telegram.

---

## 1. Purpose

Notify the owner over Telegram when:
- a client **invites** them to a job (high-signal, no filtering), or
- a new **marketplace job** appears that an LLM judges relevant to the owner's skills.

The owner's skills/profile already live in the assistant's long-term memory; that single source
drives both which marketplace queries to poll and the relevance scoring. Nothing here applies or
submits — notifications only (auto-apply is forbidden and out of scope).

### Non-goals (v1)
- Managing watch-queries via chat (queries are derived from memory automatically).
- Storing job history or cross-cycle ranking.
- Auto-applying / auto-generating proposals (the proposal-draft tool is a separate sub-project the
  owner triggers manually).
- Any web UI.

---

## 2. Context

The pieces already exist and this feature composes them:

- **Proactive push + dedup** — `repo::proactive_log::try_claim(db, kind, dedup_key) -> bool`
  (atomic claim-before-send), `repo::telegram_link::get(db) -> Option<{ chat_id }>` (owner chat),
  `telegram::client::TelegramClient::send_message(chat_id, text)`. The 5-minute proactive tick
  (`assistant/proactive/tick.rs`) uses exactly this claim-then-send pattern for financial alerts.
- **Upwork connection** — `upwork/` (sub-project 1): OAuth2, `upwork_integration` single-row token
  store, a mockable `UpworkClient` trait, and `engine::ensure_access_token` (currently private).
- **LLM text** — `llm::claude::ClaudeClient::from_env()` + `complete(system, &[Part::Text(..)])`.
- **Memory** — `assistant::memory::MemoryClient::{from_env, search}` and `render_facts_block`.

This feature adds a job/invitation source to `UpworkClient`, a `upwork/jobs.rs` module (pure
helpers + an orchestration `notify_cycle`), and a dedicated polling loop. It reuses `proactive_log`
for dedup and `TelegramClient` for delivery.

---

## 3. Components

### 3.1 `UpworkClient` extension — `backend/src/upwork/client.rs`

Add to the trait (both `HttpUpwork` and `testkit::FakeUpwork` implement them):

```rust
async fn fetch_marketplace_jobs(&self, query: &str) -> Result<Vec<MarketplaceJob>, ClientError>;
async fn fetch_invitations(&self, cursor: Option<&str>) -> Result<InvitationBatch, ClientError>;
```

New types:
```rust
pub struct MarketplaceJob {
    pub id: String,        // dedup key
    pub title: String,
    pub description: String,
    pub budget: Option<String>,
    pub url: String,
    pub skills: Vec<String>,
}
pub struct Invitation {
    pub id: String,        // dedup key
    pub job_title: String,
    pub client_note: Option<String>,
    pub url: String,
}
pub struct InvitationBatch { pub invitations: Vec<Invitation>, pub next_cursor: Option<String> }
```

`HttpUpwork` impls issue the GraphQL marketplace-search / invitation queries (field paths verified
against the live schema via a gated smoke test, as with earnings). `FakeUpwork` gains preset
vectors + recorded inputs for tests.

### 3.2 `upwork/jobs.rs` — pure helpers + orchestration

**Pure (unit-testable; no DB/network/LLM):**
- `build_query_prompt(facts: &[MemoryFact]) -> String` and `parse_queries(resp: &str, max: usize) -> Vec<String>` — derive up to `max` marketplace search keywords from skill facts; `parse_queries` tolerates extra prose / numbering and returns trimmed, deduped, non-empty terms.
- `build_scoring_prompt(jobs: &[MarketplaceJob], facts: &[MemoryFact]) -> String` and `parse_scores(resp: &str) -> Vec<JobScore>` where `JobScore { id: String, score: u8, reason: String }`; `parse_scores` tolerates missing/garbled lines (unparseable → that job is dropped, i.e. not notified).
- `format_job_alert(job: &MarketplaceJob, score: u8, reason: &str) -> String` and `format_invitation_alert(inv: &Invitation) -> String` — plain-text Telegram messages (no Markdown), including title, score/reason (jobs), budget, and URL.

**Orchestration `async fn notify_cycle(db: &Db) -> anyhow::Result<()>`:**
1. `telegram_link::get(db)` → owner `chat_id`; `None` → return Ok (no one to notify).
2. Ensure an Upwork access token (§3.3); build `HttpUpwork`. Token error → record `last_error` on `upwork_integration`, return Ok.
3. Pull skill facts: `MemoryClient::from_env()?.search("skills experience", FACT_LIMIT)` (best-effort; empty on failure).
4. Derive queries: if facts non-empty and LLM available, `build_query_prompt` → `complete` → `parse_queries(.., MAX_WATCH_QUERIES)`; on any failure → empty query list (marketplace skipped this cycle, invitations still run).
5. Fetch: for each query `fetch_marketplace_jobs` (per-query error logged, continue); `fetch_invitations(None)`.
6. Dedup + select: for each job, `try_claim(db, "upwork-job", &job.id)` → only newly-claimed jobs proceed; for each invitation, `try_claim(db, "upwork-invite", &inv.id)`.
7. Score newly-claimed jobs in one batch LLM call (`build_scoring_prompt`/`parse_scores`); keep `score >= threshold`. If the LLM is unavailable, skip job notifications this cycle (claimed jobs are already marked seen — acceptable; they won't be re-evaluated). Invitations are always sent.
8. Send each selected alert via `TelegramClient::send_message(chat_id, msg)`; per-send error logged, continue.

> Claim-before-decide note: `try_claim` marks an item "seen" the first time. A job that is seen but
> scores below threshold is intentionally never reconsidered — this bounds LLM cost and avoids
> re-notifying. This is the deliberate v1 behavior.

### 3.3 Shared token helper — `backend/src/upwork/engine.rs`

Refactor the existing private `ensure_access_token(db, cfg, key)` to `pub(crate)` so both the
earnings engine and `jobs::notify_cycle` reuse one connection's token (DRY). No behavior change.

### 3.4 Polling loop + env — `backend/src/upwork/jobs.rs`

`pub fn spawn(db: Db)` starts an independent loop on interval `UPWORK_JOBS_POLL_SECS`
(default 1800s), mirroring `upwork::engine::spawn` / google. No-op when `upwork::oauth::OAuthConfig::from_env()` is unset. Wired from `main.rs` next to `upwork`/google spawns.

Env:
- `UPWORK_JOBS_POLL_SECS` (default `1800`)
- `UPWORK_JOB_SCORE_THRESHOLD` (default `7`, range 0–10)
- `UPWORK_MAX_WATCH_QUERIES` (default `3`)

---

## 4. Data flow

```
loop (every UPWORK_JOBS_POLL_SECS):
  owner = telegram_link::get(); if none → skip
  token = ensure_access_token();  client = HttpUpwork(token)
  facts = memory.search("skills experience")
  queries = parse_queries(LLM(build_query_prompt(facts)))        # ≤ MAX_WATCH_QUERIES; [] on failure
  jobs = ⋃ client.fetch_marketplace_jobs(q) for q in queries
  invites = client.fetch_invitations()
  new_jobs    = [j for j in jobs    if try_claim("upwork-job",    j.id)]
  new_invites = [i for i in invites if try_claim("upwork-invite", i.id)]
  scores = parse_scores(LLM(build_scoring_prompt(new_jobs, facts)))   # skipped if LLM down
  for j in new_jobs where score(j) >= THRESHOLD:  send(format_job_alert(j, score, reason))
  for i in new_invites:                            send(format_invitation_alert(i))
```

No portfolio/cashflow writes. No new migration — dedup uses the existing `proactive_log` table
(new `kind` values `"upwork-job"` / `"upwork-invite"`).

---

## 5. Error handling

| Condition | Behavior |
|---|---|
| Owner not linked to Telegram | Return Ok, skip silently. |
| Upwork token error | Record `last_error` on `upwork_integration`; return Ok. |
| Memory unavailable | Empty facts → marketplace queries skipped; invitations still processed. |
| LLM unavailable (query derivation) | Empty query list → marketplace skipped this cycle; invitations still sent. |
| LLM unavailable (scoring) | Skip job notifications this cycle; invitations still sent. (Claimed jobs stay claimed.) |
| Per-query fetch error | Log + continue other queries. |
| Per-send Telegram error | Log + continue. |
| Re-run / overlap | `try_claim` is atomic → never double-notifies. |

---

## 6. Testing (TDD)

- **`parse_queries`** — extracts terms from a clean list and from messy LLM output (numbering, prose, blank lines); caps at `max`; dedups; drops empties.
- **`parse_scores`** — parses well-formed `id score reason` rows; drops unparseable/garbled lines without failing the batch.
- **`format_job_alert` / `format_invitation_alert`** — contain title, URL, and (jobs) score/reason; no Markdown; budget shown when present.
- **prompt builders** — `build_query_prompt` asks for search keywords from the facts; `build_scoring_prompt` demands a 0–10 score and "use only the provided facts/skills"; both English.
- **`notify_cycle`** with `FakeUpwork` + in-memory DB + fake LLM/Telegram seams: newly-claimed jobs above threshold are sent; a second run sends nothing (dedup); a below-threshold job is not sent; invitations are always sent; no owner link → nothing sent.

The `HttpUpwork` GraphQL field paths are validated only by a gated live smoke test (skipped by
default), as with earnings — `UPWORK_SMOKE_DB`/ignored test.

---

## 7. Out of scope (restated)

Chat-managed watch-queries, job-history storage, cross-cycle ranking, auto-apply/auto-proposal,
web UI. The design leaves room for a future job-feed consumer (e.g. the proposal tool reading a
stored marketplace job instead of a manual paste).
