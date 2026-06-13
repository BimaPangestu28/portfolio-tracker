# Capture & Triage — Design

**Date:** 2026-06-13
**Status:** Approved (design), pending implementation plan
**Phase:** Productivity roadmap — Fase 4

## Overview

Reduce capture friction. The owner can dump a quick thought, dictate a voice
note, or paste a long meeting note, and the assistant routes it intelligently:
a vague quick dump lands in a GTD-style **inbox** for later sorting; a clear
actionable item is created immediately; a multi-item note is **extracted** into
todos/events/tasks after a confirm. Voice notes are transcribed and flow through
the exact same routing as typed text.

## Goals

- **Quick-capture inbox**: dump anything ("inget beliin kado") → stored raw in an
  inbox; later "sortir inbox" → the assistant proposes a classification for every
  pending item in one batch, and on confirm creates them and clears the inbox.
- **Action-item extraction**: a longer note → assistant parses items, echoes a
  summary ("3 todo, 1 event"), and creates them after the owner confirms.
- **Voice notes**: a Telegram voice message → transcribed (OpenAI Whisper) →
  echoed back ("Aku denger: …") → routed through the normal text pipeline.
- **Smart routing**: the assistant decides per message — quick/ambiguous dump →
  inbox; clear single actionable → create directly; multi-item note → extract.

## Non-Goals (YAGNI for v1)

- No per-item interactive inbox sort (batch was chosen).
- No transcript-correction UI beyond the echoed "Aku denger: …".
- No audio handling beyond Telegram voice notes (no music/file transcription).
- "Note" outcomes route to existing long-term memory (`remember`); **no separate
  notes table**.

## Constraints / Dependencies

- **OpenAI Whisper** for transcription, reusing the existing `OPENAI_API_KEY`
  already used by `NativeLlmClient` (vision/ingestion). Per-minute cost; when the
  key is unset, voice notes degrade with a friendly "voice belum didukung" reply
  and existing behavior is unaffected.
- **Migration `0019_inbox.sql`.** On `main` the latest migration is
  `0018_upwork_project_link`, so `0019` is the next free number.
  > **Cross-branch coordination:** the Fase 2 branch (PR #54) currently carries
  > `0017_todo_priority_estimate`, which now collides with `main`'s
  > `0017_invoices`. Fase 2's migration must be renumbered (to `0020`, leaving
  > Fase 4 with `0019`) and re-checked against `origin/main` before it merges.
- **Behavioral change:** smart routing means the assistant will sometimes route a
  short message to the inbox instead of immediately creating a todo. This is the
  intended tradeoff.

## Data Model

Migration `0019_inbox.sql`:

```sql
-- Fase 4: GTD quick-capture inbox. Raw captures await batch triage.
CREATE TABLE inbox (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  content TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'pending'
    CHECK (status IN ('pending', 'sorted', 'dropped')),
  created_at TEXT NOT NULL,           -- RFC3339 +00:00
  sorted_at TEXT                      -- set when status leaves 'pending'
);

CREATE INDEX idx_inbox_pending ON inbox (status, id);
```

New repo `backend/src/repo/inbox.rs`:

- `InboxRow { id, content, status, created_at, sorted_at }`.
- `create(db, content) -> InboxRow`.
- `list_pending(db) -> Vec<InboxRow>` (ordered by id).
- `resolve(db, ids: &[i64], status: &str) -> u64` — set `status` + `sorted_at`
  for the given pending ids; returns rows affected. `status` is `sorted` or
  `dropped`.

## Components

### Inbox tools (`tools.rs` + `dispatcher.rs`)

- `capture_to_inbox` — `{ content: string }` → `inbox::create`; reply confirms
  ("dicatat ke inbox").
- `list_inbox` — `{}` → pending items with ids; "(inbox kosong)" when empty.
- `resolve_inbox` — `{ ids: [integer], status: "sorted"|"dropped" }` → mark items;
  validates status. Used after a batch sort/drop.

### Batch sort flow (prompt-driven, no dedicated sort tool)

On "sortir inbox" / "beresin inbox": the assistant calls `list_inbox`, proposes a
classification for every pending item in one message (e.g. "#1 beli kado → todo,
#2 meeting Senin jam 10 → event, #3 ide fitur → note"), and on the owner's
confirm: creates each via the existing `create_todo`/`create_event`/`create_task`/
`remember` tools, then calls `resolve_inbox(ids=[handled], status="sorted")`.
Items the owner rejects → `resolve_inbox(status="dropped")` or left pending.

### Action-item extraction (prompt-driven, no new tool)

On a multi-item note: the assistant extracts candidate items, echoes a summary
("Kebaca: 3 todo, 1 event — …"), and on confirm creates them with the existing
tools. Reuses the batch-confirm discipline.

### Smart routing (prompt in `agent.rs`)

`SYSTEM` gains guidance to choose per message:
- vague/quick single dump with no clear action → `capture_to_inbox`;
- clear single actionable ("bayar pajak besok") → create directly (todo/event/task
  as today);
- multi-item note → extract + echo + confirm;
- "apa di inbox?" / "sortir inbox" → `list_inbox` then the batch sort flow.

### Voice transcription

- **Telegram** (`telegram/client.rs` + `mod.rs`): parse the `voice` message
  variant (`file_id`, `mime_type` like `audio/ogg`). Add a routing branch: a voice
  message from a linked owner → download bytes → transcribe → echo
  "Aku denger: «transcript»" → pass the transcript to `agent::handle_message`
  (the same path as a typed message), so routing/extraction/capture all apply.
- **Transcription** (`llm/native.rs`): `NativeLlmClient::transcribe(bytes, mime) ->
  anyhow::Result<String>` calling OpenAI `POST /v1/audio/transcriptions`
  (multipart: `file` + `model=whisper-1`). Optional `WHISPER_MODEL` env (default
  `whisper-1`).
- Empty/failed transcription → friendly reply ("nggak kedengeran jelas, coba
  ketik aja ya"); never panics.

## Data Flow

- **Quick dump:** "inget beli kado" → agent (smart routing) → `capture_to_inbox`.
- **Sort:** "sortir inbox" → `list_inbox` → agent proposes batch → owner confirms →
  per item `create_*`/`remember` + `resolve_inbox(sorted)`.
- **Extraction:** paste note → agent extracts → echoes summary → confirm →
  `create_*`.
- **Voice:** voice message → download → `transcribe` → echo → `handle_message`
  (then any of the above).

## Error Handling

- `resolve_inbox` with an invalid status → error so the model self-corrects.
- `capture_to_inbox` empty content → error ("isi catatannya apa?").
- Whisper unavailable (no key) or transcription error/empty → friendly text reply;
  existing tools unaffected.
- Inbox DB failures propagate as "db error" like other repo-backed tools.

## Testing

- `inbox` repo: create → list_pending; `resolve` flips status + stamps
  `sorted_at`, leaves others pending; resolving unknown ids affects 0 rows.
- Dispatcher (with in-memory DB): `capture_to_inbox` stores; `list_inbox`
  empty/non-empty; `resolve_inbox` marks sorted/dropped and rejects bad status.
- `NativeLlmClient::transcribe` — unit-test the multipart request construction /
  response parsing at whatever seam the existing OpenAI calls are tested (mirror
  the vision-extract tests); the live HTTP call itself is not unit-tested.
- Telegram: voice message parsed into the new variant; the routing branch calls
  transcribe then `handle_message` (assert via a seam/fake where the existing
  ingest tests do).
- Tool registration test updated with the three new names.

## Open Coordination Item

- Migration `0019` for inbox. Fase 2 (PR #54) must renumber its `0017` to `0020`
  before merge (collides with `main`'s `0017_invoices`).
- `agent.rs` `SYSTEM` and `tools.rs` tool list are touched here too — same
  trivial merge-conflict point as Fase 2/Fase 3 branches.
