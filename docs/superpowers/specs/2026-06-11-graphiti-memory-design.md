# Personal Assistant — Phase 2: Long-Term Memory with Graphiti

**Date:** 2026-06-11
**Status:** Approved
**Supersedes:** the "Phase 2: notes & knowledge (LLM + keyword)" entry in
`2026-06-11-assistant-phase1-todos-reminders-design.md`. The keyword-based
notes design is replaced by a Graphiti temporal knowledge graph.

## Context

Phase 1 gave the assistant todos, reminders, and a tool-use agent loop over
Telegram. This phase adds long-term memory: the assistant should remember
facts about the owner across conversations — automatically extracted from
chat, explicitly saved on request, time-aware ("kenapa dulu aku beli BTC?"),
and relational ("tagihan apa saja yang terkait rumah?").

[Graphiti](https://github.com/getzep/graphiti) (Zep's open-source temporal
knowledge graph engine) covers exactly this: episode ingestion with
LLM-driven entity/relation extraction, bi-temporal fact tracking, and hybrid
retrieval (semantic + BM25 + graph traversal).

Decisions made during brainstorming:

- **Memory content:** auto-extraction from all chat turns, explicit notes,
  financial decision context, and entity relations — the full Graphiti use
  case.
- **Infrastructure:** self-hosted on the existing k3s cluster; full Neo4j
  accepted (resources are not a constraint).
- **Providers:** Claude (existing `ANTHROPIC_API_KEY`) for extraction;
  OpenAI `text-embedding-3-small` for embeddings (new `OPENAI_API_KEY` —
  Graphiti has no Anthropic embedder).
- **Retrieval style:** hybrid — relevant facts auto-injected into the system
  prompt every message, plus an explicit `search_memory` tool.
- **Integration shape (Approach A):** a thin custom FastAPI sidecar wrapping
  `graphiti-core`, not the generic official REST server and not MCP. Custom
  entity types are the main reason: domain-typed extraction (Person, Bill,
  Investment, Preference) beats generic extraction for this use case, and a
  ~150-line service we own is easier to debug and keep API-stable.

## Architecture

```
backend (Rust) ──HTTP──> memory-service (Python/FastAPI + graphiti-core)
                              │
                              ├──bolt──> Neo4j (graph DB, PVC on k3s)
                              ├──HTTPS──> Anthropic API (entity extraction)
                              └──HTTPS──> OpenAI API (embeddings)
```

Three new components:

1. **`memory-service/`** — new top-level directory (peer of `backend/`,
   `whatsapp-gateway/`). Python + FastAPI + `graphiti-core`. Sole owner of
   the Neo4j connection; the Rust backend never speaks to the graph DB
   directly.
2. **Neo4j** — official `neo4j:5-community` container, persistent volume,
   cluster-internal only (no ingress).
3. **`assistant/memory.rs`** — thin HTTP client in the Rust backend:
   `search()`, `ingest_turn()`, plus timeouts. Every call is wrapped so a
   memory failure can never fail a chat reply.

Boundary principle: the Rust backend knows nothing about graphs, Cypher, or
embeddings — it sends text and receives fact strings. All graph intelligence
lives in memory-service. Swapping Graphiti out later touches only
memory-service.

## Memory-Service API

| Endpoint | Body/Params | Behavior |
|---|---|---|
| `POST /episodes` | `{ "text": str, "source": "chat" \| "manual", "timestamp": iso8601 }` | Returns `202 Accepted` immediately; runs Graphiti `add_episode` (extraction + graph update) in a FastAPI background task. Callers never wait on extraction (it takes seconds and multiple LLM calls). |
| `GET /search?q=...&limit=8` | query text | Graphiti hybrid search. Response: `{ "facts": [{ "fact": str, "valid_at": iso8601 \| null, "name": str }] }` — facts as prompt-ready sentences. |
| `GET /health` | — | Verifies the Neo4j connection; used by k8s probes. |

### Custom entity types

Defined as Pydantic models in memory-service and passed to `add_episode`:

- `Person` — family, friends, colleagues, and their relations to the owner.
- `Bill` — recurring obligations (electricity, school, installments) with
  due context.
- `Investment` — investment decisions and their stated reasons over time.
- `Preference` — the owner's habits and preferences.

Graphiti still extracts generic entities beyond these; custom types only
sharpen domain extraction.

### Configuration (env)

`NEO4J_URI`, `NEO4J_USER`, `NEO4J_PASSWORD`, `ANTHROPIC_API_KEY` (extraction
model overridable), `OPENAI_API_KEY` (embeddings). Single-user app → one
constant `group_id`; no auth between services (cluster-internal network,
same trust model as the existing whatsapp-gateway).

## Rust Backend Integration

### `assistant/memory.rs`

- Base URL from `MEMORY_SERVICE_URL`; when unset, all memory features are
  silently disabled (same pattern as `TELEGRAM_BOT_TOKEN`).
- `search(query) -> Vec<MemoryFact>` — hard 2-second timeout; timeout/error
  → empty vec + `warn!`. Chat must never wait on sick memory.
- `ingest_turn(user_msg, assistant_reply)` — called AFTER the reply is
  delivered, via `tokio::spawn` fire-and-forget; failure is logged only.

### `assistant/agent.rs` changes

1. **Auto-inject:** before the loop, call `memory::search(user_msg)`. When
   non-empty, append to the system prompt:

   ```
   Known facts about the owner (from long-term memory, may be incomplete):
   - <fact> (as of <valid_at>)
   ```

2. **Ingest:** on the success path (after chat rows are stored), spawn
   `ingest_turn`. One episode per full turn:
   `"User: {user_msg}\nAssistant: {reply}"`.

### New tools (tools.rs + dispatcher.rs)

| Tool | Input | Behavior |
|---|---|---|
| `search_memory` | `{ query }` | `memory::search` with a larger limit (15); renders facts + valid_at. For explicit recall questions. |
| `remember` | `{ note }` | `POST /episodes` with `source: "manual"`. Explicit notes become high-signal episodes immediately. |

The system prompt gains one sentence telling the model these tools exist and
when to use them.

**Untouched:** photo ingest, fund-entry flows, reminder tick, web API.
Todos/reminders are NOT separately ingested as episodes in this phase — the
chat turn that created them is already ingested; entity dedup is Graphiti's
job.

## Deployment

- **Dev:** `docker-compose.yml` gains `neo4j` (with volume) and
  `memory-service` (built from `memory-service/Dockerfile`).
- **k3s:** new manifests in `k8s/` — Neo4j (Deployment + PVC + internal
  Service) and memory-service (Deployment + Service). Follows the existing
  deploy convention: manifests applied manually, CD does image bumps only.
  New secrets: `OPENAI_API_KEY`, Neo4j password.

## Failure Modes (memory down ≠ assistant down)

| Failure | Behavior |
|---|---|
| `MEMORY_SERVICE_URL` unset | Memory features fully off, no errors |
| Search timeout/error | Injection skipped, chat proceeds; `search_memory` tool returns a friendly error to the model |
| Ingest failure | Warn log; one episode lost (acceptable — not transactional data) |
| Neo4j down | memory-service `/health` fails, k8s restarts it; backend unaffected |

## Cost Awareness

Every chat turn triggers Graphiti extraction (several Claude calls per
episode) plus embeddings. At personal volume (tens of messages/day) this is
cents per day — acceptable, but not zero. The extraction model is
configurable to a cheaper tier if needed.

## Testing

- **memory-service:** pytest — API shapes and background-task wiring with
  `graphiti-core` mocked; health endpoint.
- **Rust:** unit tests for pure parts (fact-block rendering into the system
  prompt, request payload building, disabled-when-env-unset behavior);
  dispatcher tests for both new tools with memory disabled (graceful
  errors).
- **End-to-end:** manual smoke with Neo4j live — chat a fact, verify it
  surfaces in a later conversation and via `search_memory`.

## Out of Scope (this phase)

- Backfilling historical `chat_message` rows into the graph (good follow-up:
  a one-shot script).
- Graph visualization UI.
- Multi-user support.
- WhatsApp as an ingestion source.
