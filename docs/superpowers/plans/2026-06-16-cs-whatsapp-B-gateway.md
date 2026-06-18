# CS WhatsApp — Plan B: Gateway Parameterization Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the one `whatsapp-gateway` codebase run as either the owner gateway (default) or the CS gateway, selected purely by env, and add an outbound poll-and-send loop so the CS number can deliver the owner's dashboard replies.

**Architecture:** Add a `PATH_PREFIX` env (owner `""`, CS `/cs`) applied to every backend URL, a configurable `AUTH_DIR`, skip-send when the backend reply is null (the bot-silent/escalated case), and a gated outbound loop (`OUTBOUND_ENABLED=1`) that polls `${PATH_PREFIX}/whatsapp/outbound` and sends each `{jid,text}`.

**Tech Stack:** Node.js (ESM), Baileys. No new dependencies. Depends on Plan A's endpoints.

> **Worktree** `/home/bima-pangestu/Works/portfolio-tracker/.claude/worktrees/cs-chatbot`. File: `whatsapp-gateway/index.js`.

> **Plan A endpoint map (must match):**
> - Owner (PREFIX=`""`): `POST /chat/whatsapp/inbound`, `POST /whatsapp/state`, `GET /whatsapp/commands`. (No outbound.)
> - CS (PREFIX=`/cs`): `POST /cs/chat/whatsapp/inbound`, `POST /cs/whatsapp/state`, `GET /cs/whatsapp/commands`, `GET /cs/whatsapp/outbound` → `{messages:[{jid,text}]}`.
> - CS inbound returns `{reply: string | null}` — `null` means do NOT send (human took over).

---

## File Structure

- Modify: `whatsapp-gateway/index.js` — env params + null-reply skip + outbound loop.
- (No `package.json` change — same deps.)

> **Testing note:** the gateway has no JS test harness (package.json has only `start`). Verification is `node --check index.js` (syntax) + careful review. Do NOT add a test framework.

---

## Task 1: Parameterize env (path prefix, auth dir) + skip null replies

**Files:** Modify `whatsapp-gateway/index.js`.

- [ ] **Step 1: Add env params at the top**

Replace the existing top-of-file constants:

```javascript
const BACKEND = process.env.BACKEND_URL ?? "http://localhost:8080";
const GATEWAY_TOKEN = process.env.GATEWAY_TOKEN ?? "";
const AUTH_DIR = "auth_state";
const COMMAND_POLL_MS = 3000;
```

with:

```javascript
const BACKEND = process.env.BACKEND_URL ?? "http://localhost:8080";
const GATEWAY_TOKEN = process.env.GATEWAY_TOKEN ?? "";
// "" = owner gateway; "/cs" = customer-service gateway. Prefixes every backend path.
const PATH_PREFIX = process.env.PATH_PREFIX ?? "";
const AUTH_DIR = process.env.AUTH_DIR ?? "auth_state";
const COMMAND_POLL_MS = 3000;
// Outbound (proactive send) only exists on the CS backend; enable per-instance.
const OUTBOUND_ENABLED = process.env.OUTBOUND_ENABLED === "1";
const OUTBOUND_POLL_MS = 3000;
```

- [ ] **Step 2: Apply the prefix to the three existing URLs + skip null reply**

- `reportState`: change `${BACKEND}/whatsapp/state` → `` `${BACKEND}${PATH_PREFIX}/whatsapp/state` ``.
- `fetchCommand`: change `${BACKEND}/whatsapp/commands` → `` `${BACKEND}${PATH_PREFIX}/whatsapp/commands` ``.
- `forwardInbound`: change `${BACKEND}/chat/whatsapp/inbound` → `` `${BACKEND}${PATH_PREFIX}/chat/whatsapp/inbound` ``, and guard the send so a null/empty reply is not sent:

```javascript
async function forwardInbound(sock, from, text) {
  try {
    const res = await fetch(`${BACKEND}${PATH_PREFIX}/chat/whatsapp/inbound`, {
      method: "POST",
      headers: authHeaders,
      body: JSON.stringify({ from, message: text }),
    });
    if (!res.ok) { console.error("backend error", res.status); return; }
    const { reply } = await res.json();
    if (reply) await sock.sendMessage(from, { text: reply }); // null reply = bot silent / human took over
  } catch (e) {
    console.error("gateway error", e);
  }
}
```

- [ ] **Step 3: Verify syntax**

Run: `cd whatsapp-gateway && node --check index.js`
Expected: no output (valid).

- [ ] **Step 4: Commit**

```bash
git add whatsapp-gateway/index.js
git commit -m "feat(gateway): PATH_PREFIX + AUTH_DIR env, skip null reply"
```

---

## Task 2: Outbound poll-and-send loop

**Files:** Modify `whatsapp-gateway/index.js`.

- [ ] **Step 1: Add the outbound loop**

Add a function near `startCommandLoop`:

```javascript
/** Poll the backend for queued outbound messages and send them (CS gateway only). */
function startOutboundLoop() {
  setInterval(async () => {
    try {
      const res = await fetch(`${BACKEND}${PATH_PREFIX}/whatsapp/outbound`, { headers: authHeaders });
      if (!res.ok) return;
      const { messages } = await res.json();
      for (const m of messages ?? []) {
        if (!m?.jid || !m?.text) continue;
        try {
          await currentSock?.sendMessage(m.jid, { text: m.text });
        } catch (e) {
          console.error("outbound send failed", e);
        }
      }
    } catch (e) {
      console.error("fetchOutbound failed", e);
    }
  }, OUTBOUND_POLL_MS);
}
```

- [ ] **Step 2: Start it conditionally**

At the bottom of the file, after `startCommandLoop();`, add:

```javascript
if (OUTBOUND_ENABLED) startOutboundLoop();
```

- [ ] **Step 3: Verify syntax**

Run: `cd whatsapp-gateway && node --check index.js`
Expected: no output (valid).

- [ ] **Step 4: Final review + commit**

Re-read the whole file: confirm the owner path (no env set) is byte-for-byte behavior-equivalent to before (PREFIX="", AUTH_DIR="auth_state", OUTBOUND_ENABLED false → no outbound loop), and the CS path (PATH_PREFIX="/cs", AUTH_DIR set, OUTBOUND_ENABLED="1") hits the `/cs/*` routes and sends queued messages.

```bash
git add whatsapp-gateway/index.js
git commit -m "feat(gateway): outbound poll-and-send loop for CS proactive replies"
```

---

## Self-Review

**Spec coverage (Plan B):**
- One codebase, env-selected role (`PATH_PREFIX`) ✓ Task 1.
- Configurable `AUTH_DIR` ✓ Task 1.
- Null reply (bot silent) not sent ✓ Task 1.
- Outbound poll-and-send, gated so the owner gateway (no outbound endpoint) doesn't poll ✓ Task 2.
- Owner behavior unchanged when no env is set ✓ (defaults preserve current values).

**Placeholder scan:** No TBD/TODO. The full modified functions are given.

**Type/contract consistency:** Paths match Plan A exactly (`${PREFIX}/chat/whatsapp/inbound`, `${PREFIX}/whatsapp/{state,commands,outbound}`). Outbound payload shape `{messages:[{jid,text}]}` matches Plan A's `OutboundBatch`. `{reply}` null-skip matches `CsWaOut.reply: Option<String>`.

---

## Downstream

- **Plan C — Deploy:** a `cs-gateway` service/Deployment running this same image with `PATH_PREFIX=/cs`, `OUTBOUND_ENABLED=1`, `AUTH_DIR=/app/auth_state` (own volume), `GATEWAY_TOKEN=<CS token>` (backend reads it as `CS_GATEWAY_TOKEN`).
- **Plan D — Frontend:** pairing card + inbox reply box.
