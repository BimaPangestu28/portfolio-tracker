"""FastAPI surface: POST /episodes, GET /search, GET /health.

Ingestion is slow (multiple LLM calls per episode), so /episodes returns 202
immediately and extraction runs as a background task.
"""

import asyncio
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


async def _ingest(memory, lock: asyncio.Lock, text: str, source: str, timestamp: datetime) -> None:
    # graphiti's add_episode is read-modify-write (dedup reads the graph, then
    # writes); concurrent calls can both read before either writes and create
    # duplicate entities. One episode at a time, in arrival order.
    async with lock:
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

    app.state.ingest_lock = asyncio.Lock()

    @app.post("/episodes", status_code=202)
    async def add_episode(episode: EpisodeIn, background_tasks: BackgroundTasks, request: Request):
        timestamp = episode.timestamp or datetime.now(timezone.utc)
        if timestamp.tzinfo is None:
            timestamp = timestamp.replace(tzinfo=timezone.utc)
        else:
            timestamp = timestamp.astimezone(timezone.utc)
        background_tasks.add_task(
            _ingest,
            request.app.state.memory,
            request.app.state.ingest_lock,
            episode.text,
            episode.source,
            timestamp,
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
