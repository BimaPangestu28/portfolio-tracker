# ClickUp Project Assistant (Telegram) — Design

**Date:** 2026-06-13
**Status:** Approved

## Overview

Let the owner manage freelance projects and tasks from the Telegram assistant,
with ClickUp as the single source of truth. The bot adds tasks to a project,
asks which project when it's unclear, and offers to create the project in
ClickUp when it doesn't exist yet. It can also read and complete tasks, set due
dates, flag billable work, and surface due/overdue tasks in the existing
morning briefing.

The bot is the Rust backend (`portfolio-tracker`). It talks to ClickUp over the
REST API with a stored personal API token — the ClickUp MCP used during design
is a Claude-session-only tool and is NOT available to the running backend.

## Goals

A natural exchange like:

> "tambahin task bikin kontrak"
> → "Buat project mana? PT AIS atau Klien B?"
> "Klien Baru"
> → "Project 'Klien Baru' belum ada di ClickUp. Mau aku bikinin?"
> "iya"
> → "✅ Project 'Klien Baru' dibuat. Task 'bikin kontrak' ditambahkan."

works end to end, and the morning briefing lists ClickUp tasks due per client.

## Source of truth

ClickUp owns project tasks. The existing local todos/reminders/events stay for
personal, non-project items and are untouched. No two-way sync — every project
task read/write goes straight to ClickUp. A ClickUp **List** = a project; a
ClickUp **task** = a task; both live in the configured Space.

## Non-goals (v1)

- Per-task scheduled reminders (a separate ClickUp-due scheduler). Reminding is
  briefing-based and on-demand instead (Approach A). May follow later.
- Two-way sync or local mirroring of ClickUp tasks.
- Migrating existing local todos/reminders into ClickUp.
- Creating ClickUp custom fields or Spaces from code (the ClickUp API can't);
  these are one-time UI setup, read by the backend at startup.
- The invoice generator (separate, later project — this design only records the
  `Billable`/`Amount` data it will consume).

## Architecture

A new `clickup` module mirrors the existing `google` module (external REST
integration with its own client). Layers:

- **`clickup::client`** — a thin `reqwest` REST client behind a `ClickUpApi`
  trait (the seam, mirroring the `ToolModel` LLM seam). Methods: `list_lists`,
  `create_list`, `create_task`, `list_tasks`, `complete_task`,
  `get_custom_fields`. The trait lets dispatch logic be tested against a fake
  client with no network.
- **`clickup::config`** — reads `CLICKUP_API_TOKEN`, `CLICKUP_WORKSPACE_ID`
  (90182781247), `CLICKUP_SPACE_ID` (901811400643) from env at startup. When
  the token is absent, the ClickUp tools are simply not registered and the bot
  runs without them (mirrors how `telegram::spawn` no-ops without a token).
- **Assistant tools** — new schemas in `assistant::tools` and handlers in
  `assistant::dispatcher`, following the exact pattern of the recently added
  review-confirmation tools.
- **Briefing integration** — a new section in `assistant::proactive::briefing`
  that queries ClickUp for due/overdue tasks grouped by project.

## Tools (assistant agent)

| Tool | Input | Behavior |
|------|-------|----------|
| `list_projects` | — | List the Space's Lists (id, name). |
| `create_project` | `name` | Create a List. **Always confirm with the user first** (prompt rule). |
| `create_task` | `project` (name or id), `title`, `due?`, `billable?`, `amount?` | Create a task in the project's List. If `project` is missing/ambiguous, the agent asks which; if it names a non-existent project, the agent offers `create_project`. |
| `list_tasks` | `project?` or `scope` (`today`/`overdue`) | Read tasks, optionally filtered. |
| `complete_task` | `task_id` | Mark a task complete. |

Due dates parse via the existing `assistant::time::parse_tool_datetime` (WIB).
`billable`/`amount` set the ClickUp custom fields when those fields exist; when
they don't (not yet created in the UI), that part is skipped and the task is
still created — the handler says so in its result string.

## Dynamic disambiguation

Handled by the agent tool-use loop plus prompt guidance — not a hard-coded state
machine. The `SYSTEM` prompt gains a section: for "tambahin task/todo …", call
`list_projects`; if the user named no project and more than one exists, ask
which; if the named project isn't found, ask before calling `create_project`,
then `create_task`. Hard rule: **always confirm before `create_project`**
(creating a task is immediate, like local todos; creating a project is not).

## Reminders & briefing (Approach A)

No new scheduler. Two read paths:
- **On-demand:** "task hari ini?" / "apa yang overdue?" → `list_tasks` with the
  `today`/`overdue` scope.
- **Morning briefing:** `proactive::briefing` gains a "Task ClickUp jatuh tempo"
  section that queries ClickUp live and groups due/overdue tasks by project.
  When ClickUp isn't configured, the section is omitted.

## Configuration & prerequisites

- Env: `CLICKUP_API_TOKEN`, `CLICKUP_WORKSPACE_ID`, `CLICKUP_SPACE_ID`.
- One-time ClickUp UI setup (read, not created, by the backend): the Space's
  status workflow and the `Billable` (checkbox) + `Amount` (money) custom
  fields. The backend resolves the custom-field ids at startup via
  `get_custom_fields`; missing fields disable only the billable path.

## Error handling

- ClickUp API failures (auth, 4xx/5xx, network) map to an `Err(String)` from the
  tool, which the agent relays to the user in plain language — never panics the
  loop (same contract as existing dispatcher tools).
- Missing/invalid token at startup → ClickUp tools unregistered; bot continues.
- An unknown project name is not an error — it's the trigger for the
  offer-to-create flow.

## Testing

- `ClickUpApi` trait with a fake implementation drives dispatcher tests:
  create_task into a known project, create_project then create_task, list/
  complete, and the "project not found" path returning the offer.
- Pure tests for due-date parsing reuse `parse_tool_datetime` coverage.
- Briefing section formatting tested with a fake client returning due/overdue
  tasks; empty/unconfigured case omits the section.
- Tool-schema tests extend the existing exact-names assertion (as the review
  tools did).

## Implementation phases (for the plan)

1. `clickup` client + trait + config + `list_projects`/`create_project`/
   `create_task` tools + disambiguation prompt.
2. `list_tasks` (incl. `today`/`overdue`) + `complete_task` + due dates.
3. Billable/amount custom-field resolution and wiring into `create_task`.
4. Morning-briefing ClickUp section.
