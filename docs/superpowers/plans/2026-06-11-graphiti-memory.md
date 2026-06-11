# Long-Term Memory with Graphiti (Phase 2) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give the Telegram assistant long-term memory via a thin Python sidecar wrapping Graphiti (temporal knowledge graph on Neo4j), with auto-injected facts plus `search_memory`/`remember` tools.

**Architecture:** New `memory-service/` (FastAPI + graphiti-core, sole Neo4j owner, custom entity types) exposes `POST /episodes`, `GET /search`, `GET /health`. The Rust backend gains `assistant/memory.rs` (failure-tolerant HTTP client): facts are searched before each agent call and appended to the system prompt; the finished turn is ingested fire-and-forget; two new tools route through the same client. Memory down ≠ assistant down, everywhere.

**Tech Stack:** Python 3.12 + FastAPI + graphiti-core 0.29.x (Anthropic extraction, OpenAI `text-embedding-3-small` embeddings) + Neo4j 5.26; Rust side uses existing reqwest/serde — no new crates.

**Spec:** `docs/superpowers/specs/2026-06-11-graphiti-memory-design.md`

**PREREQUISITE:** PR #41 (`feat/assistant-phase1-todos-reminders`) must be merged first. Branch this work from the updated `main`. Rust tasks modify `assistant/agent.rs`, `assistant/tools.rs`, `assistant/dispatcher.rs` as they exist after that merge (state also visible on the PR branch).

**Conventions:**
- Rust commands run from `backend/`; Python commands run from `memory-service/` using `.venv/bin/...`.
- Python tests never touch Neo4j or LLM APIs — graphiti is mocked/faked at the `GraphMemory` boundary.
- Rust tests never set `MEMORY_SERVICE_URL` — the disabled path is the test default; HTTP behavior is exercised only by pure-function tests + manual smoke.
- Commit after every task.

---

### Task 1: memory-service scaffold + entity types

**Files:**
- Create: `memory-service/.gitignore`
- Create: `memory-service/requirements.txt`
- Create: `memory-service/requirements-dev.txt`
- Create: `memory-service/app/__init__.py`
- Create: `memory-service/app/entities.py`
- Create: `memory-service/tests/__init__.py`
- Create: `memory-service/tests/test_entities.py`

- [ ] **Step 1: Scaffold files**

`memory-service/.gitignore`:

```
.venv/
__pycache__/
*.pyc
.pytest_cache/
```

`memory-service/requirements.txt`:

```
fastapi==0.115.*
uvicorn[standard]==0.34.*
graphiti-core[anthropic]==0.29.*
```

`memory-service/requirements-dev.txt`:

```
pytest==8.*
pytest-asyncio==0.25.*
httpx==0.28.*
```

`memory-service/app/__init__.py` and `memory-service/tests/__init__.py`: empty files.

- [ ] **Step 2: Set up the venv**

Run:
```bash
cd memory-service && python3 -m venv .venv && .venv/bin/pip install -r requirements.txt -r requirements-dev.txt
```
Expected: installs cleanly. (graphiti-core pulls neo4j driver, anthropic, openai.)

- [ ] **Step 3: Write the failing test**

`memory-service/tests/test_entities.py`:

```python
"""The ENTITY_TYPES dict is what gets passed to Graphiti's add_episode."""

from app.entities import ENTITY_TYPES


def test_defines_the_four_domain_entity_types():
    assert sorted(ENTITY_TYPES.keys()) == ["Bill", "Investment", "Person", "Preference"]


def test_entity_models_have_descriptions_and_no_name_field():
    # Graphiti owns the `name` attribute on entities; custom models must not
    # redefine it, and each model's docstring guides the extraction LLM.
    for label, model in ENTITY_TYPES.items():
        assert model.__doc__, f"{label} needs a docstring (used as extraction guidance)"
        assert "name" not in model.model_fields, f"{label} must not define 'name'"


def test_entity_fields_are_optional():
    # Extraction may find an entity without filling every attribute.
    for label, model in ENTITY_TYPES.items():
        instance = model()
        assert instance is not None, f"{label} must be constructible with no args"
```

- [ ] **Step 4: Run test to verify it fails**

Run: `cd memory-service && .venv/bin/pytest tests/test_entities.py -v`
Expected: FAIL — `ModuleNotFoundError: app.entities`.

- [ ] **Step 5: Implement**

`memory-service/app/entities.py`:

```python
"""Custom entity types for Graphiti extraction.

Each docstring doubles as guidance for the extraction LLM. Models must NOT
define a `name` field — Graphiti owns entity names and summaries.
"""

from pydantic import BaseModel, Field


class Person(BaseModel):
    """A person in the owner's life: family member, friend, or colleague."""

    relation_to_owner: str | None = Field(
        None, description="Relationship to the owner, e.g. 'anak', 'istri', 'teman kantor'"
    )


class Bill(BaseModel):
    """A recurring financial obligation such as electricity, school fees, or an installment."""

    cadence: str | None = Field(None, description="How often it recurs, e.g. 'monthly'")
    due_hint: str | None = Field(None, description="When it is typically due, e.g. 'tanggal 25'")


class Investment(BaseModel):
    """An investment decision or holding, and the reasoning behind it."""

    action: str | None = Field(None, description="What was done: buy, sell, hold, rebalance")
    reason: str | None = Field(None, description="The stated reason for the decision")


class Preference(BaseModel):
    """A habit or preference of the owner."""

    context: str | None = Field(None, description="Where this preference applies")


ENTITY_TYPES = {
    "Person": Person,
    "Bill": Bill,
    "Investment": Investment,
    "Preference": Preference,
}
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cd memory-service && .venv/bin/pytest tests/test_entities.py -v`
Expected: 3 PASS.

- [ ] **Step 7: Commit**

```bash
git add memory-service
git commit -m "feat(memory): scaffold memory-service with domain entity types"
```

---

### Task 2: GraphMemory wrapper

**Files:**
- Create: `memory-service/app/memory.py`
- Create: `memory-service/tests/test_memory.py`

- [ ] **Step 1: Write the failing tests**

`memory-service/tests/test_memory.py`:

```python
"""GraphMemory maps our 3-call surface onto graphiti-core.

The graphiti instance is injected so tests never need Neo4j or API keys.
"""

from datetime import datetime, timezone
from types import SimpleNamespace
from unittest.mock import AsyncMock

import pytest

from app.entities import ENTITY_TYPES
from app.memory import GROUP_ID, GraphMemory


def fake_graphiti():
    g = AsyncMock()
    g.search.return_value = []
    return g


@pytest.mark.asyncio
async def test_add_episode_maps_chat_source():
    g = fake_graphiti()
    memory = GraphMemory(graphiti=g)
    ts = datetime(2026, 6, 11, 9, 0, tzinfo=timezone.utc)
    await memory.add_episode("User: halo\nAssistant: halo!", "chat", ts)

    kwargs = g.add_episode.call_args.kwargs
    assert kwargs["episode_body"] == "User: halo\nAssistant: halo!"
    assert kwargs["source_description"] == "telegram chat turn"
    assert kwargs["reference_time"] == ts
    assert kwargs["group_id"] == GROUP_ID
    assert kwargs["entity_types"] is ENTITY_TYPES


@pytest.mark.asyncio
async def test_add_episode_maps_manual_source():
    g = fake_graphiti()
    memory = GraphMemory(graphiti=g)
    ts = datetime(2026, 6, 11, 9, 0, tzinfo=timezone.utc)
    await memory.add_episode("ingat: paspor di laci kiri", "manual", ts)
    assert g.add_episode.call_args.kwargs["source_description"] == "explicit note"


@pytest.mark.asyncio
async def test_search_maps_edges_to_fact_dicts():
    g = fake_graphiti()
    g.search.return_value = [
        SimpleNamespace(
            fact="Noah is the owner's son",
            valid_at=datetime(2026, 6, 1, tzinfo=timezone.utc),
            name="IS_SON_OF",
        ),
        SimpleNamespace(fact="owner pays electricity monthly", valid_at=None, name="PAYS"),
    ]
    memory = GraphMemory(graphiti=g)
    facts = await memory.search("anak", limit=5)

    assert g.search.call_args.kwargs["num_results"] == 5
    assert g.search.call_args.kwargs["group_ids"] == [GROUP_ID]
    assert facts[0] == {
        "fact": "Noah is the owner's son",
        "valid_at": "2026-06-01T00:00:00+00:00",
        "name": "IS_SON_OF",
    }
    assert facts[1]["valid_at"] is None


@pytest.mark.asyncio
async def test_healthy_reflects_driver_state():
    g = fake_graphiti()
    memory = GraphMemory(graphiti=g)
    assert await memory.healthy() is True
    g.driver.execute_query.side_effect = RuntimeError("down")
    assert await memory.healthy() is False
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd memory-service && .venv/bin/pytest tests/test_memory.py -v`
Expected: FAIL — `ModuleNotFoundError: app.memory`.

- [ ] **Step 3: Implement**

`memory-service/app/memory.py`:

```python
"""Thin wrapper owning the Graphiti instance — the only Neo4j touchpoint."""

import logging
import os
from datetime import datetime

from graphiti_core import Graphiti
from graphiti_core.embedder.openai import OpenAIEmbedder, OpenAIEmbedderConfig
from graphiti_core.llm_client.anthropic_client import AnthropicClient
from graphiti_core.llm_client.config import LLMConfig
from graphiti_core.nodes import EpisodeType

from .entities import ENTITY_TYPES

logger = logging.getLogger(__name__)

# Single-user app: one constant graph partition.
GROUP_ID = "owner"


def build_graphiti() -> Graphiti:
    """Production wiring: Claude for extraction, OpenAI for embeddings."""
    llm_config = LLMConfig(
        api_key=os.environ["ANTHROPIC_API_KEY"],
        model=os.environ.get("MEMORY_LLM_MODEL", "claude-haiku-4-5-20251001"),
    )
    return Graphiti(
        os.environ["NEO4J_URI"],
        os.environ["NEO4J_USER"],
        os.environ["NEO4J_PASSWORD"],
        llm_client=AnthropicClient(config=llm_config),
        embedder=OpenAIEmbedder(
            config=OpenAIEmbedderConfig(
                api_key=os.environ["OPENAI_API_KEY"],
                embedding_model="text-embedding-3-small",
            )
        ),
    )


class GraphMemory:
    """Our 3-call surface over graphiti-core; inject `graphiti` in tests."""

    def __init__(self, graphiti: Graphiti | None = None) -> None:
        self.graphiti = graphiti or build_graphiti()

    async def setup(self) -> None:
        await self.graphiti.build_indices_and_constraints()

    async def add_episode(self, text: str, source: str, timestamp: datetime) -> None:
        await self.graphiti.add_episode(
            name=f"{source}-{timestamp.isoformat()}",
            episode_body=text,
            source=EpisodeType.message if source == "chat" else EpisodeType.text,
            source_description="telegram chat turn" if source == "chat" else "explicit note",
            reference_time=timestamp,
            group_id=GROUP_ID,
            entity_types=ENTITY_TYPES,
        )

    async def search(self, query: str, limit: int) -> list[dict]:
        edges = await self.graphiti.search(query, group_ids=[GROUP_ID], num_results=limit)
        return [
            {
                "fact": edge.fact,
                "valid_at": edge.valid_at.isoformat() if edge.valid_at else None,
                "name": edge.name,
            }
            for edge in edges
        ]

    async def healthy(self) -> bool:
        try:
            await self.graphiti.driver.execute_query("RETURN 1")
            return True
        except Exception:
            logger.exception("neo4j health check failed")
            return False
```

API-drift guard (graphiti-core is pinned to 0.29.* but check on install): if `Graphiti.search` rejects `num_results`/`group_ids` keyword names, or `add_episode` rejects `entity_types`, or `driver.execute_query` doesn't exist, READ the installed signatures (`.venv/bin/python -c "import inspect, graphiti_core; print(inspect.signature(graphiti_core.Graphiti.search))"` etc.) and adapt the parameter names ONLY — the test assertions then change to match. For health, the fallback is `await self.graphiti.driver.client.verify_connectivity()`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd memory-service && .venv/bin/pytest tests/test_memory.py -v`
Expected: 4 PASS.

- [ ] **Step 5: Commit**

```bash
git add memory-service
git commit -m "feat(memory): add GraphMemory wrapper over graphiti-core"
```

---

### Task 3: FastAPI app + endpoints

**Files:**
- Create: `memory-service/app/main.py`
- Create: `memory-service/tests/test_api.py`

- [ ] **Step 1: Write the failing tests**

`memory-service/tests/test_api.py`:

```python
"""Endpoint behavior with a fake memory — no Neo4j, no LLM calls."""

from datetime import datetime, timezone

from fastapi.testclient import TestClient

from app.main import create_app


class FakeMemory:
    def __init__(self, healthy: bool = True):
        self._healthy = healthy
        self.episodes: list[tuple[str, str, datetime]] = []
        self.searches: list[tuple[str, int]] = []

    async def add_episode(self, text, source, timestamp):
        self.episodes.append((text, source, timestamp))

    async def search(self, query, limit):
        self.searches.append((query, limit))
        return [{"fact": "Noah is the owner's son", "valid_at": None, "name": "IS_SON_OF"}]

    async def healthy(self):
        return self._healthy


def client_with(memory):
    return TestClient(create_app(memory=memory))


def test_post_episode_returns_202_and_ingests_in_background():
    memory = FakeMemory()
    response = client_with(memory).post(
        "/episodes",
        json={"text": "User: halo\nAssistant: halo!", "source": "chat"},
    )
    assert response.status_code == 202
    # TestClient runs background tasks before returning.
    assert len(memory.episodes) == 1
    text, source, timestamp = memory.episodes[0]
    assert source == "chat"
    assert timestamp.tzinfo is not None  # timestamp defaulted to now (UTC)


def test_post_episode_honors_explicit_timestamp_and_manual_source():
    memory = FakeMemory()
    response = client_with(memory).post(
        "/episodes",
        json={"text": "ingat: paspor di laci", "source": "manual", "timestamp": "2026-06-11T09:00:00+07:00"},
    )
    assert response.status_code == 202
    _, source, timestamp = memory.episodes[0]
    assert source == "manual"
    assert timestamp == datetime(2026, 6, 11, 2, 0, tzinfo=timezone.utc)


def test_post_episode_rejects_bad_source_and_empty_text():
    memory = FakeMemory()
    assert client_with(memory).post("/episodes", json={"text": "x", "source": "webhook"}).status_code == 422
    assert client_with(memory).post("/episodes", json={"text": "", "source": "chat"}).status_code == 422
    assert memory.episodes == []


def test_search_returns_fact_list():
    memory = FakeMemory()
    response = client_with(memory).get("/search", params={"q": "anak", "limit": 5})
    assert response.status_code == 200
    assert response.json() == {
        "facts": [{"fact": "Noah is the owner's son", "valid_at": None, "name": "IS_SON_OF"}]
    }
    assert memory.searches == [("anak", 5)]


def test_search_defaults_limit_and_requires_query():
    memory = FakeMemory()
    client = client_with(memory)
    assert client.get("/search", params={"q": "anak"}).status_code == 200
    assert memory.searches == [("anak", 8)]
    assert client.get("/search").status_code == 422


def test_health_reflects_memory_state():
    assert client_with(FakeMemory(healthy=True)).get("/health").status_code == 200
    assert client_with(FakeMemory(healthy=False)).get("/health").status_code == 503
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd memory-service && .venv/bin/pytest tests/test_api.py -v`
Expected: FAIL — `ModuleNotFoundError: app.main`.

- [ ] **Step 3: Implement**

`memory-service/app/main.py`:

```python
"""FastAPI surface: POST /episodes, GET /search, GET /health.

Ingestion is slow (multiple LLM calls per episode), so /episodes returns 202
immediately and extraction runs as a background task.
"""

import logging
from contextlib import asynccontextmanager
from datetime import datetime, timezone

from fastapi import BackgroundTasks, FastAPI, HTTPException, Query, Request
from pydantic import BaseModel, Field

from .memory import GraphMemory

logger = logging.getLogger(__name__)


class EpisodeIn(BaseModel):
    text: str = Field(min_length=1)
    source: str = Field(pattern="^(chat|manual)$")
    timestamp: datetime | None = None


async def _ingest(memory, text: str, source: str, timestamp: datetime) -> None:
    try:
        await memory.add_episode(text, source, timestamp)
    except Exception:
        # One lost episode is acceptable; a crashed worker is not.
        logger.exception("episode ingestion failed")


@asynccontextmanager
async def _lifespan(app: FastAPI):
    memory = GraphMemory()
    await memory.setup()
    app.state.memory = memory
    yield


def create_app(memory=None) -> FastAPI:
    """Tests inject a fake memory; production builds GraphMemory on startup."""
    if memory is None:
        app = FastAPI(lifespan=_lifespan)
    else:
        app = FastAPI()
        app.state.memory = memory

    @app.post("/episodes", status_code=202)
    async def add_episode(episode: EpisodeIn, background_tasks: BackgroundTasks, request: Request):
        timestamp = episode.timestamp or datetime.now(timezone.utc)
        if timestamp.tzinfo is None:
            timestamp = timestamp.replace(tzinfo=timezone.utc)
        else:
            timestamp = timestamp.astimezone(timezone.utc)
        background_tasks.add_task(
            _ingest, request.app.state.memory, episode.text, episode.source, timestamp
        )
        return {"status": "accepted"}

    @app.get("/search")
    async def search(
        request: Request,
        q: str = Query(min_length=1),
        limit: int = Query(8, ge=1, le=50),
    ):
        facts = await request.app.state.memory.search(q, limit)
        return {"facts": facts}

    @app.get("/health")
    async def health(request: Request):
        if await request.app.state.memory.healthy():
            return {"status": "ok"}
        raise HTTPException(status_code=503, detail="neo4j unreachable")

    return app


app = create_app()
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd memory-service && .venv/bin/pytest -v`
Expected: 13 PASS (3 entities + 4 memory + 6 api).

- [ ] **Step 5: Commit**

```bash
git add memory-service
git commit -m "feat(memory): add FastAPI endpoints for episodes, search, health"
```

---

### Task 4: memory-service Dockerfile

**Files:**
- Create: `memory-service/Dockerfile`

- [ ] **Step 1: Write the Dockerfile**

```dockerfile
FROM python:3.12-slim

WORKDIR /app

COPY requirements.txt .
RUN pip install --no-cache-dir -r requirements.txt

COPY app ./app

# Disable graphiti's anonymous telemetry for a private personal service.
ENV GRAPHITI_TELEMETRY_ENABLED=false

EXPOSE 8000
CMD ["uvicorn", "app.main:app", "--host", "0.0.0.0", "--port", "8000"]
```

- [ ] **Step 2: Verify it builds**

Run: `cd memory-service && docker build -t portfolio-memory:dev .`
Expected: builds successfully. (Skip if Docker is unavailable locally; CI/compose will exercise it — note that in the report.)

- [ ] **Step 3: Commit**

```bash
git add memory-service/Dockerfile
git commit -m "feat(memory): add memory-service Dockerfile"
```

---

### Task 5: Rust memory client (`assistant/memory.rs`)

**Files:**
- Create: `backend/src/assistant/memory.rs`
- Modify: `backend/src/assistant/mod.rs` (add `pub mod memory;`)

- [ ] **Step 1: Write the failing tests**

Create `backend/src/assistant/memory.rs` with doc comment, imports, and tests:

```rust
//! HTTP client for the memory-service (Graphiti sidecar). Every call is
//! failure-tolerant: memory being down must never break chat.

use serde::Deserialize;
use std::time::Duration;

#[cfg(test)]
mod tests {
    use super::*;

    fn fact(text: &str, valid_at: Option<&str>) -> MemoryFact {
        MemoryFact {
            fact: text.to_string(),
            valid_at: valid_at.map(String::from),
            name: "REL".to_string(),
        }
    }

    #[test]
    fn no_facts_renders_nothing() {
        assert_eq!(render_facts_block(&[]), "");
    }

    #[test]
    fn facts_render_as_a_prompt_block_with_dates() {
        let block = render_facts_block(&[
            fact("Noah is the owner's son", Some("2026-06-01T00:00:00+00:00")),
            fact("owner pays electricity monthly", None),
        ]);
        assert!(block.contains("Known facts about the owner"), "{block}");
        assert!(block.contains("- Noah is the owner's son (as of 2026-06-01)"), "{block}");
        assert!(block.contains("- owner pays electricity monthly\n"), "{block}");
        // The block must lead with a blank line so it appends cleanly to the
        // system prompt.
        assert!(block.starts_with("\n\n"), "{block:?}");
    }

    #[test]
    fn short_valid_at_values_render_unsliced() {
        let block = render_facts_block(&[fact("x", Some("2026"))]);
        assert!(block.contains("(as of 2026)"), "{block}");
    }

    #[test]
    fn from_env_is_none_when_unset() {
        // Tests never set MEMORY_SERVICE_URL (see plan conventions), so the
        // disabled path is the default everywhere in the suite.
        assert!(MemoryClient::from_env().is_none());
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Add `pub mod memory;` to `backend/src/assistant/mod.rs`.

Run: `cd backend && cargo test assistant::memory`
Expected: COMPILE ERROR — `MemoryFact`, `render_facts_block`, `MemoryClient` not found.

- [ ] **Step 3: Implement**

Insert between imports and tests:

```rust
/// One fact returned by the memory-service search endpoint.
#[derive(Debug, Deserialize)]
pub struct MemoryFact {
    pub fact: String,
    pub valid_at: Option<String>,
    #[allow(dead_code)]
    pub name: String,
}

#[derive(Debug, Deserialize)]
struct SearchResponse {
    facts: Vec<MemoryFact>,
}

/// Hard ceiling on memory lookups — chat must never wait on sick memory.
const SEARCH_TIMEOUT: Duration = Duration::from_secs(2);
/// Ingest posts run in spawned tasks; allow more slack.
const INGEST_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone)]
pub struct MemoryClient {
    base_url: String,
    client: reqwest::Client,
}

impl MemoryClient {
    /// None when MEMORY_SERVICE_URL is unset — all memory features disabled.
    pub fn from_env() -> Option<Self> {
        let base_url = std::env::var("MEMORY_SERVICE_URL").ok()?;
        let client = reqwest::Client::builder().timeout(SEARCH_TIMEOUT).build().ok()?;
        Some(Self { base_url: base_url.trim_end_matches('/').to_string(), client })
    }

    /// Search the graph; any failure degrades to "no facts" with a warning.
    pub async fn search(&self, query: &str, limit: u32) -> Vec<MemoryFact> {
        let url = format!("{}/search", self.base_url);
        let result = self
            .client
            .get(&url)
            .query(&[("q", query), ("limit", &limit.to_string())])
            .send()
            .await;
        match result {
            Ok(resp) if resp.status().is_success() => match resp.json::<SearchResponse>().await {
                Ok(body) => body.facts,
                Err(e) => {
                    tracing::warn!("memory search: unreadable response: {e}");
                    Vec::new()
                }
            },
            Ok(resp) => {
                tracing::warn!("memory search: status {}", resp.status());
                Vec::new()
            }
            Err(e) => {
                tracing::warn!("memory search failed: {e}");
                Vec::new()
            }
        }
    }

    /// Post one episode ("chat" turn or "manual" note). The service replies
    /// 202 immediately; extraction happens on its side in the background.
    pub async fn add_episode(&self, text: &str, source: &str) -> Result<(), String> {
        let url = format!("{}/episodes", self.base_url);
        let body = serde_json::json!({ "text": text, "source": source });
        match self.client.post(&url).timeout(INGEST_TIMEOUT).json(&body).send().await {
            Ok(resp) if resp.status().is_success() => Ok(()),
            Ok(resp) => Err(format!("memory service returned {}", resp.status())),
            Err(e) => Err(format!("memory service unreachable: {e}")),
        }
    }
}

/// Render facts as a system-prompt block; empty input renders nothing.
/// Dates are truncated to the day — the model doesn't need timestamps.
pub fn render_facts_block(facts: &[MemoryFact]) -> String {
    if facts.is_empty() {
        return String::new();
    }
    let mut out = String::from(
        "\n\nKnown facts about the owner (from long-term memory, may be incomplete):\n",
    );
    for f in facts {
        out.push_str(&format!("- {}", f.fact));
        if let Some(valid_at) = &f.valid_at {
            let date = valid_at.get(..10).unwrap_or(valid_at);
            out.push_str(&format!(" (as of {date})"));
        }
        out.push('\n');
    }
    out
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd backend && cargo test assistant::memory`
Expected: 4 PASS. Full `cargo test`: measure baseline first (expected 278 post-PR-#41); expect +4.

- [ ] **Step 5: Commit**

```bash
git add backend/src/assistant
git commit -m "feat(assistant): add failure-tolerant memory-service client"
```

---

### Task 6: Tool definitions for `search_memory` and `remember`

**Files:**
- Modify: `backend/src/assistant/tools.rs`

- [ ] **Step 1: Update the failing tests**

In `backend/src/assistant/tools.rs`, the test `defines_all_phase1_tools_with_schemas` asserts the exact name list. Update the expected list (and the test name) to:

```rust
    #[test]
    fn defines_all_tools_with_schemas() {
        let defs = definitions();
        let names: Vec<&str> = defs
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert_eq!(
            names,
            vec![
                "create_todo", "list_todos", "complete_todo",
                "create_reminder", "list_reminders", "cancel_reminder",
                "get_portfolio_summary", "search_memory", "remember",
            ]
        );
        for tool in defs.as_array().unwrap() {
            assert!(tool["description"].is_string(), "{} needs a description", tool["name"]);
            assert_eq!(tool["input_schema"]["type"], "object");
        }
    }
```

And extend `required_fields_are_marked` with:

```rust
        assert_eq!(find("search_memory")["input_schema"]["required"], serde_json::json!(["query"]));
        assert_eq!(find("remember")["input_schema"]["required"], serde_json::json!(["note"]));
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd backend && cargo test assistant::tools`
Expected: FAIL — names list mismatch.

- [ ] **Step 3: Implement**

In `definitions()`, append after the `get_portfolio_summary` object (inside the array):

```rust
        {
            "name": "search_memory",
            "description": "Search the owner's long-term memory (facts learned from past conversations and notes). Use for recall questions like 'kapan aku bilang soal X?' or when past context would change the answer.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "What to look for, in natural language" }
                },
                "required": ["query"]
            }
        },
        {
            "name": "remember",
            "description": "Save an explicit note to the owner's long-term memory. Use when the user asks you to remember something ('ingat ya ...', 'catat: ...' when it is a fact rather than a task).",
            "input_schema": {
                "type": "object",
                "properties": {
                    "note": { "type": "string", "description": "The fact to remember, as a standalone sentence" }
                },
                "required": ["note"]
            }
        }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd backend && cargo test assistant::tools`
Expected: 2 PASS.

- [ ] **Step 5: Commit**

```bash
git add backend/src/assistant/tools.rs
git commit -m "feat(assistant): define search_memory and remember tool schemas"
```

---

### Task 7: Dispatcher handlers for the memory tools

**Files:**
- Modify: `backend/src/assistant/dispatcher.rs`

- [ ] **Step 1: Write the failing tests**

Add to the tests module in `backend/src/assistant/dispatcher.rs`:

```rust
    #[tokio::test]
    async fn search_memory_requires_query_and_errors_when_unconfigured() {
        let db = mem_db().await;
        let err = dispatch(&db, "search_memory", &serde_json::json!({})).await.unwrap_err();
        assert!(err.contains("query"), "{err}");
        // Tests never set MEMORY_SERVICE_URL, so memory is unconfigured here.
        let err = dispatch(&db, "search_memory", &serde_json::json!({ "query": "anak" }))
            .await
            .unwrap_err();
        assert!(err.contains("not configured"), "{err}");
    }

    #[tokio::test]
    async fn remember_requires_note_and_errors_when_unconfigured() {
        let db = mem_db().await;
        let err = dispatch(&db, "remember", &serde_json::json!({})).await.unwrap_err();
        assert!(err.contains("note"), "{err}");
        let err = dispatch(&db, "remember", &serde_json::json!({ "note": "paspor di laci" }))
            .await
            .unwrap_err();
        assert!(err.contains("not configured"), "{err}");
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd backend && cargo test assistant::dispatcher`
Expected: the two new tests FAIL — `dispatch` hits the `unknown tool` arm ("unknown tool: search_memory" does not contain "query").

- [ ] **Step 3: Implement**

In the `dispatch` match, add before the `_` arm:

```rust
        "search_memory" => search_memory(input).await,
        "remember" => remember(input).await,
```

Add the handlers (after `portfolio_summary`):

```rust
/// How many facts an explicit memory search returns to the model — larger
/// than the auto-inject limit because the user asked for recall.
const TOOL_SEARCH_LIMIT: u32 = 15;

async fn search_memory(input: &serde_json::Value) -> Result<String, String> {
    let query = str_arg(input, "query").ok_or("missing required argument 'query'")?;
    let Some(memory) = super::memory::MemoryClient::from_env() else {
        return Err("long-term memory is not configured".into());
    };
    let facts = memory.search(query, TOOL_SEARCH_LIMIT).await;
    if facts.is_empty() {
        return Ok("no memories found for that query".into());
    }
    let mut out = String::new();
    for f in facts {
        out.push_str(&format!("- {}", f.fact));
        if let Some(valid_at) = &f.valid_at {
            out.push_str(&format!(" (as of {valid_at})"));
        }
        out.push('\n');
    }
    Ok(out)
}

async fn remember(input: &serde_json::Value) -> Result<String, String> {
    let note = str_arg(input, "note").ok_or("missing required argument 'note'")?;
    let Some(memory) = super::memory::MemoryClient::from_env() else {
        return Err("long-term memory is not configured".into());
    };
    memory
        .add_episode(note, "manual")
        .await
        .map_err(|e| format!("could not save the note: {e}"))?;
    Ok("noted — saved to long-term memory".into())
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd backend && cargo test assistant::dispatcher`
Expected: 13 PASS (11 existing + 2 new).

- [ ] **Step 5: Commit**

```bash
git add backend/src/assistant/dispatcher.rs
git commit -m "feat(assistant): dispatch search_memory and remember tools"
```

---

### Task 8: Agent integration — auto-inject + ingest

**Files:**
- Modify: `backend/src/assistant/agent.rs`

- [ ] **Step 1: Write the failing tests**

Add to the tests module in `backend/src/assistant/agent.rs`:

```rust
    #[test]
    fn compose_system_without_facts_is_just_the_prompt() {
        let system = compose_system("2026-06-11T15:00:00+07:00", &[]);
        assert_eq!(system, system_prompt("2026-06-11T15:00:00+07:00"));
    }

    #[test]
    fn compose_system_appends_the_facts_block() {
        let facts = vec![crate::assistant::memory::MemoryFact {
            fact: "Noah is the owner's son".into(),
            valid_at: None,
            name: "IS_SON_OF".into(),
        }];
        let system = compose_system("2026-06-11T15:00:00+07:00", &facts);
        assert!(system.starts_with(&system_prompt("2026-06-11T15:00:00+07:00")), "{system}");
        assert!(system.contains("Known facts about the owner"), "{system}");
        assert!(system.contains("- Noah is the owner's son"), "{system}");
    }

    #[test]
    fn system_prompt_mentions_the_memory_tools() {
        let prompt = system_prompt("2026-06-11T15:00:00+07:00");
        assert!(prompt.contains("search_memory"), "{prompt}");
        assert!(prompt.contains("remember"), "{prompt}");
    }
```

Note: `MemoryFact` has no test constructor restrictions — its fields are pub.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd backend && cargo test assistant::agent`
Expected: COMPILE ERROR — `compose_system` not found (and the SYSTEM-prompt test fails once it compiles).

- [ ] **Step 3: Implement**

(a) Extend the `SYSTEM` const: append this sentence inside the existing string (before the closing quote, after the plain-text-messenger rules):

```rust
 You have long-term memory: relevant known facts about the owner may be listed \
below — treat them as context, not unquestionable truth. Use the search_memory \
tool for explicit recall questions, and the remember tool when the user asks \
you to remember a fact.
```

(b) Add the inject limit and `compose_system` next to `system_prompt`:

```rust
/// How many facts are auto-injected into the system prompt per message.
const INJECT_FACT_LIMIT: u32 = 8;

/// Full system prompt: persona + current time + any long-term-memory facts.
fn compose_system(now_wib: &str, facts: &[super::memory::MemoryFact]) -> String {
    format!("{}{}", system_prompt(now_wib), super::memory::render_facts_block(facts))
}
```

(c) In `handle_message`, replace the `let system = system_prompt(&now_wib);` line with:

```rust
    let memory = super::memory::MemoryClient::from_env();
    let facts = match &memory {
        Some(client) => client.search(user_msg, INJECT_FACT_LIMIT).await,
        None => Vec::new(),
    };
    let system = compose_system(&now_wib, &facts);
```

(d) Add the shared exit helper (above `handle_message`):

```rust
/// Persist the finished turn and (when memory is configured) ingest it as an
/// episode, fire-and-forget — ingestion must never delay or fail the reply.
async fn store_and_ingest(
    db: &Db,
    memory: Option<super::memory::MemoryClient>,
    channel: &str,
    user_msg: &str,
    reply: &str,
) -> anyhow::Result<()> {
    crate::repo::chat::add(db, "user", user_msg, channel).await?;
    crate::repo::chat::add(db, "assistant", reply, channel).await?;
    if let Some(client) = memory {
        let episode = format!("User: {user_msg}\nAssistant: {reply}");
        tokio::spawn(async move {
            if let Err(e) = client.add_episode(&episode, "chat").await {
                tracing::warn!("memory ingest failed: {e}");
            }
        });
    }
    Ok(())
}
```

(e) Replace ALL THREE exit points' `chat::add` pairs with the helper (`memory` is `Clone`, so pass `memory.clone()` at each):

- unusable-response fallback:
  ```rust
                store_and_ingest(db, memory.clone(), channel, user_msg, NO_TEXT_REPLY).await?;
                return Ok(NO_TEXT_REPLY.to_string());
  ```
- final text reply:
  ```rust
            store_and_ingest(db, memory.clone(), channel, user_msg, &reply).await?;
            return Ok(reply);
  ```
- iteration cap:
  ```rust
    store_and_ingest(db, memory, channel, user_msg, ITERATION_CAP_REPLY).await?;
    Ok(ITERATION_CAP_REPLY.to_string())
  ```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd backend && cargo test assistant::agent`
Expected: 11 PASS (8 existing + 3 new — existing ScriptedModel tests are unaffected because `MEMORY_SERVICE_URL` is never set in tests, so `memory` is `None` and behavior is unchanged). Full `cargo test`: expect +5 over Task 7's count.

- [ ] **Step 5: Commit**

```bash
git add backend/src/assistant/agent.rs
git commit -m "feat(assistant): inject memory facts into prompt and ingest chat turns"
```

---

### Task 9: docker-compose wiring

**Files:**
- Modify: `docker-compose.yml`

- [ ] **Step 1: Add the services**

In `docker-compose.yml`, add to the `backend.environment` map:

```yaml
      # Optional: enables long-term memory when the memory-service is up.
      MEMORY_SERVICE_URL: http://memory-service:8000
```

Add two services after `gateway`:

```yaml
  neo4j:
    image: neo4j:5.26-community
    restart: unless-stopped
    environment:
      NEO4J_AUTH: neo4j/${NEO4J_PASSWORD:?NEO4J_PASSWORD is required}
    volumes:
      - neo4j_data:/data
    expose:
      - "7687"

  memory-service:
    build: ./memory-service
    restart: unless-stopped
    depends_on:
      - neo4j
    environment:
      NEO4J_URI: bolt://neo4j:7687
      NEO4J_USER: neo4j
      NEO4J_PASSWORD: ${NEO4J_PASSWORD:?NEO4J_PASSWORD is required}
      ANTHROPIC_API_KEY: ${ANTHROPIC_API_KEY:?ANTHROPIC_API_KEY is required}
      OPENAI_API_KEY: ${OPENAI_API_KEY:?OPENAI_API_KEY is required}
      MEMORY_LLM_MODEL: ${MEMORY_LLM_MODEL:-claude-haiku-4-5-20251001}
    expose:
      - "8000"
    healthcheck:
      test: ["CMD-SHELL", "python -c \"import urllib.request; urllib.request.urlopen('http://localhost:8000/health')\""]
      interval: 30s
      timeout: 5s
      retries: 5
      start_period: 30s
```

Add `neo4j_data:` to the top-level `volumes:` map.

Also update `.env.production.example` (it sits next to docker-compose.yml; if the file doesn't exist, skip): add `OPENAI_API_KEY=`, `NEO4J_PASSWORD=`, `MEMORY_LLM_MODEL=` lines.

- [ ] **Step 2: Validate**

Run: `docker compose config -q`
Expected: exits 0 (with a `.env` present; otherwise expect only missing-var errors for the new `NEO4J_PASSWORD`/`OPENAI_API_KEY`, which proves the wiring parses).

- [ ] **Step 3: Commit**

```bash
git add docker-compose.yml .env.production.example
git commit -m "feat(deploy): add neo4j and memory-service to compose stack"
```

---

### Task 10: k8s manifests

**Files:**
- Create: `k8s/50-neo4j.yaml`
- Create: `k8s/60-memory-service.yaml`
- Modify: `k8s/10-backend.yaml` (env addition)
- Modify: `k8s/secret.example.yaml` (new keys)

- [ ] **Step 1: Write the Neo4j manifest**

`k8s/50-neo4j.yaml`:

```yaml
apiVersion: v1
kind: PersistentVolumeClaim
metadata:
  name: neo4j-data
  namespace: portfolio
spec:
  accessModes: [ReadWriteOnce]
  resources:
    requests:
      storage: 5Gi
---
apiVersion: apps/v1
kind: Deployment
metadata:
  name: neo4j
  namespace: portfolio
spec:
  replicas: 1
  # Single-writer graph store on a ReadWriteOnce volume.
  strategy:
    type: Recreate
  selector:
    matchLabels:
      app: neo4j
  template:
    metadata:
      labels:
        app: neo4j
    spec:
      containers:
        - name: neo4j
          image: neo4j:5.26-community
          ports:
            - containerPort: 7687
          env:
            # Format: "neo4j/<password>" — must match NEO4J_PASSWORD below.
            - name: NEO4J_AUTH
              valueFrom:
                secretKeyRef:
                  name: portfolio-secrets
                  key: NEO4J_AUTH
          volumeMounts:
            - name: data
              mountPath: /data
          readinessProbe:
            tcpSocket:
              port: 7687
            initialDelaySeconds: 20
            periodSeconds: 10
          livenessProbe:
            tcpSocket:
              port: 7687
            initialDelaySeconds: 60
            periodSeconds: 20
      volumes:
        - name: data
          persistentVolumeClaim:
            claimName: neo4j-data
---
apiVersion: v1
kind: Service
metadata:
  name: neo4j
  namespace: portfolio
spec:
  selector:
    app: neo4j
  ports:
    - port: 7687
      targetPort: 7687
```

- [ ] **Step 2: Write the memory-service manifest**

`k8s/60-memory-service.yaml`:

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: memory-service
  namespace: portfolio
spec:
  replicas: 1
  selector:
    matchLabels:
      app: memory-service
  template:
    metadata:
      labels:
        app: memory-service
    spec:
      imagePullSecrets:
        - name: ghcr-creds
      containers:
        - name: memory-service
          image: ghcr.io/bimapangestu28/portfolio-memory:latest
          imagePullPolicy: Always
          ports:
            - containerPort: 8000
          env:
            - name: NEO4J_URI
              value: "bolt://neo4j:7687"
            - name: NEO4J_USER
              value: "neo4j"
            - name: NEO4J_PASSWORD
              valueFrom:
                secretKeyRef:
                  name: portfolio-secrets
                  key: NEO4J_PASSWORD
            - name: ANTHROPIC_API_KEY
              valueFrom:
                secretKeyRef:
                  name: portfolio-secrets
                  key: ANTHROPIC_API_KEY
            - name: OPENAI_API_KEY
              valueFrom:
                secretKeyRef:
                  name: portfolio-secrets
                  key: OPENAI_API_KEY
            - name: MEMORY_LLM_MODEL
              value: "claude-haiku-4-5-20251001"
            - name: GRAPHITI_TELEMETRY_ENABLED
              value: "false"
          readinessProbe:
            httpGet:
              path: /health
              port: 8000
            initialDelaySeconds: 10
            periodSeconds: 15
          livenessProbe:
            httpGet:
              path: /health
              port: 8000
            initialDelaySeconds: 30
            periodSeconds: 30
---
apiVersion: v1
kind: Service
metadata:
  name: memory-service
  namespace: portfolio
spec:
  selector:
    app: memory-service
  ports:
    - port: 8000
      targetPort: 8000
```

- [ ] **Step 3: Wire the backend env**

In `k8s/10-backend.yaml`, add to the backend container `env` list (after the `TELEGRAM_BOT_TOKEN` entry):

```yaml
            # Optional: enables long-term memory when the memory-service is deployed.
            - name: MEMORY_SERVICE_URL
              value: "http://memory-service:8000"
```

In `k8s/secret.example.yaml`, add example entries for the three new keys following the file's existing format: `OPENAI_API_KEY`, `NEO4J_PASSWORD`, and `NEO4J_AUTH` (with a comment that `NEO4J_AUTH` must be `neo4j/<same password as NEO4J_PASSWORD>`).

- [ ] **Step 4: Validate**

Run: `kubectl apply --dry-run=client -f k8s/50-neo4j.yaml -f k8s/60-memory-service.yaml -f k8s/10-backend.yaml`
Expected: all resources validate (client-side; no cluster access needed). If kubectl is unavailable, note it in the report.

- [ ] **Step 5: Commit**

```bash
git add k8s
git commit -m "feat(deploy): add neo4j and memory-service k8s manifests"
```

---

### Task 11: Full verification + manual smoke

- [ ] **Step 1: Full test suites**

Run: `cd backend && cargo test` — expect ALL pass (Phase-1 baseline 278 + 11 new = 289; trust the measured baseline over this arithmetic).
Run: `cd memory-service && .venv/bin/pytest` — expect 13 passed.
Run: `cd backend && cargo build` — expect no warnings.

- [ ] **Step 2: Local end-to-end smoke (needs Docker + real API keys)**

1. `docker compose up -d neo4j memory-service` (with `NEO4J_PASSWORD`, `ANTHROPIC_API_KEY`, `OPENAI_API_KEY` in `.env`).
2. `curl -fsS localhost:<mapped>/health` → `{"status":"ok"}` (or `kubectl port-forward` style mapping via compose `ports` temporarily).
3. `curl -X POST localhost:<mapped>/episodes -H 'content-type: application/json' -d '{"text":"User: anakku Noah mulai les piano tiap Sabtu\nAssistant: oke, dicatat!","source":"chat"}'` → 202. Wait ~30s (extraction).
4. `curl 'localhost:<mapped>/search?q=les%20piano'` → facts mentioning Noah/piano.
5. Run the backend with `MEMORY_SERVICE_URL` pointed at the service; in Telegram say something containing a personal fact, then in a NEW conversation ask about it ("anakku les apa?") — the reply should use the remembered fact.
6. Ask "ingat ya: paspor ada di laci kiri meja kerja" → expect the `remember` tool to fire; later "di mana pasporku?" → `search_memory` or injected facts answer it.

- [ ] **Step 3: Commit any verification fixes**

```bash
git status   # should be clean
```

---

## Self-Review Notes

- **Spec coverage:** sidecar API (Tasks 1-3), Dockerfile (4), Rust client + degradation (5), tools (6-7), inject + ingest at every exit (8), compose (9), k8s + secrets (10), testing strategy + manual smoke (11). Out-of-scope items (backfill, graph UI, multi-user, WhatsApp ingest) have no tasks, as intended.
- **Type consistency:** `MemoryFact { fact, valid_at, name }` (Task 5) matches the sidecar response shape (Task 3) and the agent test usage (Task 8); `MemoryClient::from_env() -> Option`, `search(&self, &str, u32) -> Vec<MemoryFact>`, `add_episode(&self, &str, &str) -> Result<(), String>` are used identically in Tasks 7-8; `GraphMemory` method names match between `main.py` and `test_api.py`'s FakeMemory.
- **Known judgment calls:** (1) memory tools are always defined even when memory is unconfigured — the dispatcher returns a friendly error the model can relay; conditional tool lists aren't worth the complexity for a personal app. (2) All three agent exits ingest (including cap/fallback replies) — the user message itself carries memory value. (3) graphiti-core API drift is guarded by an explicit verify-and-adapt instruction in Task 2, scoped to parameter names only.
