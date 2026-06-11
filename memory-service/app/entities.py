"""Custom entity types for Graphiti extraction.

Each docstring doubles as guidance for the extraction LLM. Models must NOT
redefine any attribute Graphiti owns on EntityNode: attributes, created_at,
group_id, labels, name, name_embedding, summary, uuid. The test suite runs
graphiti's validate_entity_types() to enforce this for every future field.
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
    """A standing habit or preference of the owner, e.g. 'suka briefing singkat',
    'gajian tanggal 25', 'hindari saham berisiko tinggi'. Not a one-off decision
    (that is an Investment) and not a recurring payment (that is a Bill)."""

    context: str | None = Field(None, description="Where this preference applies")


ENTITY_TYPES = {
    "Person": Person,
    "Bill": Bill,
    "Investment": Investment,
    "Preference": Preference,
}
