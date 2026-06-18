# CS WhatsApp — Plan D: Frontend Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A "CS WhatsApp" pairing card to connect the CS number (QR/status, separate from Noah), and a reply box in the CS Inbox so the owner can answer a customer over WhatsApp.

**Architecture:** Reuse the existing WhatsApp-connect pattern (`useWhatsappStatus`/`Connect`/`Disconnect` + `WhatsAppPage.tsx`) pointed at `/cs/whatsapp/*`, surfaced as a `CsWhatsAppPage` under the "Admin (CS)" nav. Add a `useReplyConversation` mutation and a reply textarea to `CsInboxPage` that calls `/cs/admin/conversations/:id/reply` and refetches the transcript.

**Tech Stack:** React, TypeScript (strict), React Query, Zod. No new deps. Depends on Plan A endpoints.

> **Worktree** `/home/bima-pangestu/Works/portfolio-tracker/.claude/worktrees/cs-chatbot`, frontend `frontend/`. `npm run build` runs `tsc -b` (strict). Tests: `npx vitest run <path>`.

> **Existing references (from research):**
> - `schemas.ts`: `WhatsappStatusSchema = z.object({ status: z.enum([...]), qr: nullable, number: nullable })`.
> - `hooks.ts` (~line 209): `useWhatsappStatus` (GET `/whatsapp/status`, `refetchInterval: 2000`), `useConnectWhatsapp` (POST `/whatsapp/connect`), `useDisconnectWhatsapp` (POST `/whatsapp/disconnect`).
> - `pages/WhatsAppPage.tsx`: QR display + status + connect/disconnect.
> - `pages/CsInboxPage.tsx`: left = escalations + conversations; right = transcript + resolve.

---

## File Structure

- Modify: `frontend/src/api/hooks.ts` — CS WhatsApp hooks + `useReplyConversation`.
- Create: `frontend/src/pages/CsWhatsAppPage.tsx` — pairing card (mirror `WhatsAppPage`).
- Modify: `frontend/src/pages/CsInboxPage.tsx` — reply box.
- Modify: `frontend/src/App.tsx` — route for the CS WhatsApp page.
- Modify: `frontend/src/components/AppShell.tsx` — nav item under "Admin (CS)".
- Modify: `frontend/src/api/hooks.cs.test.tsx` — a hook test.

---

## Task 1: Hooks

**Files:** Modify `frontend/src/api/hooks.ts`.

- [ ] **Step 1: Add the hooks** (reuse `WhatsappStatusSchema`; mirror the existing whatsapp hooks exactly but with `/cs/whatsapp/*` paths and a `cs-whatsapp-status` query key)

```ts
export const useCsWhatsappStatus = () =>
  useQuery({
    queryKey: ["cs-whatsapp-status"],
    queryFn: () => api.get("/cs/whatsapp/status", WhatsappStatusSchema),
    refetchInterval: 2000,
  });

export const useConnectCsWhatsapp = () => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => api.post("/cs/whatsapp/connect", z.unknown(), {}),
    onSuccess: () => { qc.invalidateQueries({ queryKey: ["cs-whatsapp-status"] }); },
  });
};

export const useDisconnectCsWhatsapp = () => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => api.post("/cs/whatsapp/disconnect", z.unknown(), {}),
    onSuccess: () => { qc.invalidateQueries({ queryKey: ["cs-whatsapp-status"] }); },
  });
};

export const useReplyConversation = () =>
  useInvalidatingMutation(
    (a: { id: number; text: string }) => api.post(`/cs/admin/conversations/${a.id}/reply`, z.unknown(), { text: a.text }),
    ["cs-conversations"],
  );
```

> **Note:** `WhatsappStatusSchema` is already imported in `hooks.ts` (used by the owner hooks). Confirm; reuse it, don't redefine. `useReplyConversation` invalidates `cs-conversations`; the page will also invalidate the specific transcript query (Task 3).

- [ ] **Step 2: Typecheck + commit**

Run: `cd frontend && npx tsc -b 2>&1 | tail -8`

```bash
git add frontend/src/api/hooks.ts
git commit -m "feat(cs-wa): frontend hooks (CS whatsapp status/connect/disconnect + reply)"
```

---

## Task 2: CS WhatsApp pairing page

**Files:** Create `frontend/src/pages/CsWhatsAppPage.tsx`.

- [ ] **Step 1: Implement by mirroring `WhatsAppPage.tsx`**

READ `frontend/src/pages/WhatsAppPage.tsx` first and copy its exact structure/markup (QR rendering, status display, connect/disconnect buttons, the design-system classes it uses). Produce `CsWhatsAppPage.tsx` identical EXCEPT:
- Use `useCsWhatsappStatus`, `useConnectCsWhatsapp`, `useDisconnectCsWhatsapp` instead of the owner hooks.
- Title/copy: "CS WhatsApp" (this is the customer-service number, separate from your personal Noah number).
- If `WhatsAppPage` renders the QR from a data-URL or a string, render the same way from the CS status `qr` field.

The component must be self-contained and typecheck under strict TS. Do not invent classes — use whatever `WhatsAppPage` uses.

- [ ] **Step 2: Typecheck + commit**

Run: `cd frontend && npx tsc -b 2>&1 | tail -8`

```bash
git add frontend/src/pages/CsWhatsAppPage.tsx
git commit -m "feat(cs-wa): CS WhatsApp pairing page"
```

---

## Task 3: CS Inbox reply box

**Files:** Modify `frontend/src/pages/CsInboxPage.tsx`.

- [ ] **Step 1: Add a reply box to the transcript panel**

READ the current `CsInboxPage.tsx`. In the transcript panel (the right side, shown when a conversation is `selected`), below the message list, add a reply composer:
- local state `const [reply, setReply] = useState("")`
- `const sendReply = useReplyConversation();`
- access to the transcript query so you can refetch on success — the page uses `useCsTranscript(selected)`; capture it as a variable (e.g. `const transcript = useCsTranscript(selected)`) if not already, and call `transcript.refetch()` (or invalidate `["cs-transcript", selected]` via `useQueryClient`) after a successful reply.
- markup (use the page's existing classes; this is illustrative):

```tsx
{selected != null && (
  <div className="flex items-center" style={{ gap: 8, padding: "8px 16px", borderTop: "1px solid #e5e7eb" }}>
    <input
      className="input"
      style={{ flex: 1 }}
      placeholder="Balas ke pelanggan..."
      value={reply}
      onChange={(e) => setReply(e.target.value)}
      onKeyDown={(e) => { if (e.key === "Enter") doReply(); }}
    />
    <button
      className="btn btn-primary btn-sm"
      disabled={sendReply.isPending || !reply.trim()}
      onClick={doReply}
    >
      Kirim
    </button>
  </div>
)}
```
with:

```tsx
const doReply = () => {
  if (selected == null || !reply.trim()) return;
  sendReply.mutate(
    { id: selected, text: reply.trim() },
    {
      onSuccess: () => { setReply(""); transcript.refetch(); toast.success("Balasan terkirim"); },
      onError: (e) => toast.error((e as Error).message),
    },
  );
};
```

> **Note:** `toast` from `sonner` is already used on the page. For a WhatsApp conversation this reply is delivered to the customer; for a web conversation it is stored only (the visitor sees it if they reopen the widget). A small hint in the UI ("balasan WhatsApp dikirim langsung; web tersimpan") is optional.

- [ ] **Step 2: Typecheck + commit**

Run: `cd frontend && npx tsc -b 2>&1 | tail -8`

```bash
git add frontend/src/pages/CsInboxPage.tsx
git commit -m "feat(cs-wa): reply box in CS inbox"
```

---

## Task 4: Route + nav + hook test

**Files:** Modify `frontend/src/App.tsx`, `frontend/src/components/AppShell.tsx`, `frontend/src/api/hooks.cs.test.tsx`.

- [ ] **Step 1: Route** — in `App.tsx`, import `CsWhatsAppPage` (match the file's import style) and add inside the `AppShell` group:

```tsx
<Route path="cs/admin/whatsapp" element={<CsWhatsAppPage />} />
```

- [ ] **Step 2: Nav** — in `AppShell.tsx`, add to the "Admin (CS)" group's `items` (reuse an imported icon, e.g. `MessageCircle` from lucide-react; import it if not present):

```tsx
{ to: "/cs/admin/whatsapp", label: "CS WhatsApp", icon: MessageCircle },
```

- [ ] **Step 3: Hook test** — add to `frontend/src/api/hooks.cs.test.tsx` (MSW pattern already in the file):

```tsx
test("useCsWhatsappStatus fetches connection status", async () => {
  stubFetch({ status: "qr", qr: "data:image/png;base64,AAAA", number: null });
  const { result } = renderHook(() => useCsWhatsappStatus(), { wrapper });
  await waitFor(() => expect(result.current.isSuccess).toBe(true));
  expect(result.current.data?.status).toBe("qr");
});
```
> If the file uses the shared MSW server (not `stubFetch`), add a handler for `GET /api/cs/whatsapp/status` instead, matching the file's existing approach. Import `useCsWhatsappStatus`.

- [ ] **Step 4: Full verification + commit**

Run: `cd frontend && npx tsc -b 2>&1 | tail -12 && npx vitest run src/api/hooks.cs.test.tsx 2>&1 | tail -6 && npm run build 2>&1 | tail -6`
Expected: no type errors; hook tests pass; full build (SPA + widget) succeeds.

```bash
git add frontend/src/App.tsx frontend/src/components/AppShell.tsx frontend/src/api/hooks.cs.test.tsx
git commit -m "feat(cs-wa): CS WhatsApp route + nav + hook test"
```

---

## Self-Review

**Spec coverage (Plan D):**
- Pairing card for the CS number, separate from Noah ✓ Tasks 1,2,4.
- Reply box in CS Inbox → `/cs/admin/conversations/:id/reply`, refetches transcript ✓ Tasks 1,3.
- Hooks reuse the existing pattern + schema ✓ Task 1.

**Placeholder scan:** No TBD/TODO. Tasks 2 & 3 instruct READ-then-mirror against the real `WhatsAppPage`/`CsInboxPage` (exact classes/markup must match the codebase, not be invented).

**Consistency:** Paths match Plan A (`/cs/whatsapp/{status,connect,disconnect}`, `/cs/admin/conversations/:id/reply`). `WhatsappStatusSchema` shape matches the backend `WaStatusView`. Reply body `{text}` matches Plan A's `ReplyIn`.

---

## Done

After Plan D, Phase 2 is complete: customers reach the CS bot on a dedicated WhatsApp number; the bot answers or escalates; the owner pairs the number and replies to escalated conversations from the dashboard, delivered over WhatsApp.

**To go live (WhatsApp):** set `CS_GATEWAY_TOKEN` (backend + cs-gateway), deploy the `cs-gateway`, open the "CS WhatsApp" card and scan the QR with the CS number.
