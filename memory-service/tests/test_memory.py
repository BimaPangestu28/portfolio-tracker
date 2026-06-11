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
