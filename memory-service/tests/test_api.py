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


def test_failed_ingestion_is_swallowed_not_propagated():
    class ExplodingMemory(FakeMemory):
        async def add_episode(self, text, source, timestamp):
            raise RuntimeError("neo4j hiccup")

    memory = ExplodingMemory()
    # TestClient runs the background task before returning; the exception must
    # be logged and swallowed, never bubble into the response.
    response = client_with(memory).post(
        "/episodes", json={"text": "x", "source": "chat"}
    )
    assert response.status_code == 202
    assert memory.episodes == []
