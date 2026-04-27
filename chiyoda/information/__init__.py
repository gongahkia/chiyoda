"""
Information layer for ITED framework.

Models heterogeneous information propagation, belief states,
and Shannon entropy across agent populations during evacuation.
"""

from chiyoda.information.field import BeliefVector, InformationField
from chiyoda.information.propagation import GossipModel
from chiyoda.information.entropy import (
    agent_entropy,
    global_entropy,
    belief_accuracy,
    information_efficiency,
)
from chiyoda.information.interventions import (
    InformationInterventionConfig,
    InterventionEvent,
    InterventionMessage,
    InterventionPolicy,
    create_intervention_policy,
)
from chiyoda.information.llm import (
    GeneratedEvacuationMessage,
    LLMGenerationRecord,
    LLMMessageCache,
    LLMMessageRequest,
    OpenAIResponsesGenerator,
    ReplayOnlyGenerator,
    TemplateLLMGenerator,
    ValidationResult,
    load_openai_api_key,
    validate_generated_message,
)

__all__ = [
    "BeliefVector",
    "InformationField",
    "GossipModel",
    "agent_entropy",
    "global_entropy",
    "belief_accuracy",
    "information_efficiency",
    "InformationInterventionConfig",
    "InterventionEvent",
    "InterventionMessage",
    "InterventionPolicy",
    "create_intervention_policy",
    "GeneratedEvacuationMessage",
    "LLMGenerationRecord",
    "LLMMessageCache",
    "LLMMessageRequest",
    "OpenAIResponsesGenerator",
    "ReplayOnlyGenerator",
    "TemplateLLMGenerator",
    "ValidationResult",
    "load_openai_api_key",
    "validate_generated_message",
]
