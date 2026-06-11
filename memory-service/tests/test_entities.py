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


def test_graphiti_accepts_the_entity_types():
    # Enforce the REAL constraint (no field may collide with EntityNode's
    # attributes), not just the hand-maintained `name` check above. Pure
    # function — no Neo4j or LLM involved.
    from graphiti_core.utils.ontology_utils.entity_types_utils import validate_entity_types

    assert validate_entity_types(ENTITY_TYPES)
