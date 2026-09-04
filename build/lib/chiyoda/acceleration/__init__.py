from .backends import (
    AccelerationBackend,
    JuliaAccelerationBackend,
    PythonAccelerationBackend,
    create_acceleration_backend,
)

__all__ = [
    "AccelerationBackend",
    "ChiyodaParallelRLEnv",
    "ChiyodaRLEnv",
    "JuliaAccelerationBackend",
    "PythonAccelerationBackend",
    "create_acceleration_backend",
    "create_rl_env",
]


def __getattr__(name: str):
    if name in {"ChiyodaRLEnv", "ChiyodaParallelRLEnv", "create_rl_env"}:
        from chiyoda.acceleration import rl_env

        return getattr(rl_env, name)
    raise AttributeError(name)
