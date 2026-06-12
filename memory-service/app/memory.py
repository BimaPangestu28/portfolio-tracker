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
    """Production wiring: DeepSeek for extraction, OpenAI for embeddings.

    Extraction runs over DeepSeek's Anthropic-Messages-compatible endpoint, so
    Graphiti's AnthropicClient is reused unchanged — only the base URL, model,
    and key differ. DeepSeek has no embedding model, so embeddings stay on
    OpenAI. To revert to Claude, set LLM_BASE_URL=https://api.anthropic.com and
    MEMORY_LLM_MODEL to a claude-* id.
    """
    llm_config = LLMConfig(
        api_key=os.environ["ANTHROPIC_API_KEY"],
        base_url=os.environ.get("LLM_BASE_URL", "https://api.deepseek.com/anthropic"),
        model=os.environ.get("MEMORY_LLM_MODEL", "deepseek-v4-flash"),
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
        """Build Neo4j indices and constraints required by Graphiti."""
        await self.graphiti.build_indices_and_constraints()

    async def add_episode(self, text: str, source: str, timestamp: datetime) -> None:
        """Store a conversation turn or manual note as a Graphiti episode.

        @param text - Raw episode body content.
        @param source - Either "chat" (telegram turn) or "manual" (explicit note).
        @param timestamp - Reference time for the episode.
        """
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
        """Search the knowledge graph and return serializable fact dicts.

        @param query - Natural-language search query.
        @param limit - Maximum number of edges to return.
        @returns List of dicts with keys: fact, valid_at (ISO string or None), name.
        """
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
        """Return True if Neo4j is reachable, False otherwise."""
        try:
            await self.graphiti.driver.execute_query("RETURN 1")
            return True
        except Exception:
            logger.exception("neo4j health check failed")
            return False
