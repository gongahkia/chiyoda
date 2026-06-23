"""Environment models: layout, exits, multi-hazard physics, obstacles."""

from chiyoda.environment.exits import Exit
from chiyoda.environment.hazards import Hazard
from chiyoda.environment.layout import Layout

__all__ = [
    "Layout",
    "Exit",
    "Hazard",
    "ChiyodaRLEnv",
    "ChiyodaParallelRLEnv",
    "create_rl_env",
]


def __getattr__(name: str):
    if name in {"ChiyodaRLEnv", "ChiyodaParallelRLEnv", "create_rl_env"}:
        from chiyoda.acceleration import rl_env

        return getattr(rl_env, name)
    raise AttributeError(name)
