# CS Chatbot — Phase 2: WhatsApp Channel Design

**Date:** 2026-06-16
**Status:** Approved design, plans to follow
**Scope:** Add a dedicated customer-service WhatsApp number to the Phase-1 CS bot, with owner reply-from-dashboard. Builds on the merged/PR'd Phase 1 (`feat/cs-chatbot`).

## Goal

Customers can reach the CS bot over WhatsApp on a **separate number** from the owner's personal Noah number. The bot answers from the same brain (KB + tools + escalation); when a conversation is escalated, the bot goes silent and the owner replies from the CS Inbox dashboard, with the reply delivered to the customer over WhatsApp.

## Reuse

The Phase-1 CS brain is channel-agnostic and reused as-is: `cs::agent::handle_message`, `cs_conversation`/`cs_message`, the tool dispatcher, and escalation. The existing `whatsapp-gateway` (Baileys) code is parameterized to run a second instance. The dashboard WhatsApp-connect pattern is mirrored for pairing the CS number.

## Key decisions (from brainstorming)

| Decision | Choice |
|---|---|
| Number | Separate dedicated CS number (2nd gateway instance) |
| Escalation reply | Owner replies from CS Inbox → delivered to customer via WhatsApp (needs outbound/proactive-send) |
| Deploy | Wire a `cs-gateway` into docker-compose + k8s |
| Pairing | "CS WhatsApp" card in the dashboard (QR/status), separate from Noah |
| Conversation model | One ongoing thread per WA contact (find-or-create by JID); `visitor_phone` from JID, name null until known |
| Bot vs human | Once a conversation is `needs_human`, the bot goes **silent**; only owner inbox replies go out |
| Gateway auth | Separate `CS_GATEWAY_TOKEN` (isolated from the Noah gateway) |
| Owner reply role | Stored as `assistant` messages (no role-enum migration) |

## Architecture

```
Customer WhatsApp ──▶ CS gateway (2nd Baileys: own number, AUTH_DIR, CS_GATEWAY_TOKEN, PATH_PREFIX=/cs/whatsapp)
                          │ POST /cs/whatsapp/inbound {from, message}
                          ▼
              conversation_find_or_create_wa(from)   (channel='whatsapp', visitor_phone=from)
                          │ store user message
              status == 'bot'  ─────▶ cs::agent::handle_message → reply  (returned inline; gateway sends it)
              status == 'needs_human' / 'resolved' ─▶ store only; NO bot reply (owner has taken over)
                          ▲
   Owner in CS Inbox ──▶ POST /cs/admin/conversations/:id/reply {text}
                          │ store assistant message; if channel=='whatsapp', enqueue outbound(jid, text)
                          ▼
              CS gateway polls GET /cs/whatsapp/outbound ──▶ sock.sendMessage(jid, text) ──▶ Customer
```

## Components (4 plans)

### Plan A — Backend WhatsApp channel
- **Migration `0024`**: add `wa_jid TEXT` (nullable) + index to `cs_conversation`.
- **Repo**: `conversation_find_or_create_wa(db, jid, phone) -> ConversationRow` (one ongoing thread per JID).
- **State**: a second `WaState` instance (`cs_wa`) on `AppState`, plus an **outbound queue** (`Arc<Mutex<VecDeque<(jid, text)>>>` or held in the CS state). Construct in `main.rs`.
- **Gateway-tier endpoints** (CS gateway token): `POST /cs/whatsapp/inbound`, `POST /cs/whatsapp/state`, `GET /cs/whatsapp/commands`, `GET /cs/whatsapp/outbound` (drains the queue).
- **Protected-tier endpoints** (JWT, dashboard): `GET /cs/whatsapp/status`, `POST /cs/whatsapp/connect`, `POST /cs/whatsapp/disconnect`, and `POST /cs/admin/conversations/:id/reply`.
- **Inbound logic**: find-or-create conversation, store the user message; if `status=='bot'` run the agent and return the reply (gateway sends inline); else store only and return no reply (bot silent). Escalation already flips status to `needs_human`, so the bot self-silences after escalating.
- **Reply logic**: store an `assistant` message; if the conversation channel is `whatsapp`, enqueue an outbound `(wa_jid, text)`; if `web`, store only.

### Plan B — Gateway (Node)
- Parameterize `whatsapp-gateway/index.js`: read a path prefix + token from env (`PATH_PREFIX` default `/`, `GATEWAY_TOKEN`/`CS_GATEWAY_TOKEN`, `AUTH_DIR`, `BACKEND_URL`), so one codebase runs as the Noah gateway OR the CS gateway by env alone.
- Add an **outbound poll-and-send** loop: poll `GET {prefix}/whatsapp/outbound`, send each `{jid, text}` via `sock.sendMessage`. Keep the existing inbound + commands + state behavior.

### Plan C — Deploy
- `docker-compose.prod.yml`: a `cs-gateway` service (same image as `gateway`, env: `CS_GATEWAY_TOKEN`, `AUTH_DIR=/auth-cs`, `PATH_PREFIX=/cs/whatsapp`, `BACKEND_URL`), with its own auth-state volume.
- k8s: a CS-gateway Deployment + PVC mirroring the existing WhatsApp-gateway manifests, with its own secret/env.

### Plan D — Frontend
- A **"CS WhatsApp" pairing card** in Settings: mirror the existing WhatsApp-connect component but pointed at `/cs/whatsapp/{status,connect,disconnect}` — QR + connection status for the CS number, separate from Noah.
- A **reply box** in `CsInboxPage`: for a selected conversation, a textarea + send button → `POST /cs/admin/conversations/:id/reply`; render `assistant` messages (bot + owner) in the transcript. (For WhatsApp conversations the reply reaches the customer; for web it's stored only.)

## Error handling
- Outbound send failure on the gateway → retry/keep in queue or log + drop (avoid infinite loops); the queue drain endpoint should only remove messages the gateway confirms it pulled.
- Inbound for an unpaired/disconnected CS number still records the message (so nothing is lost); the agent runs only if clients are configured.
- Reply to a `web` conversation: stored, but the visitor only sees it if the widget refetches history (documented limitation; not a WA concern).

## Testing
- Repo: `conversation_find_or_create_wa` (creates once, returns same on repeat; sets phone).
- Inbound handler: bot-status → agent runs; needs_human → stored, no reply (gated). Per-JID isolation (two JIDs → two conversations).
- Outbound queue: enqueue/drain ordering; reply endpoint enqueues for WA, not for web.
- Gateway token isolation (CS token ≠ Noah token).
- Second `WaState` is independent of the owner's.

## Out of scope
- Live web takeover (web reply still out-of-band / history-refetch).
- Multi-tenant. Voice/media on the CS WA number (text only, like the current owner gateway).
