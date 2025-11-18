from __future__ import annotations

from dataclasses import dataclass, field
from typing import List


@dataclass
class Group:
    member_ids: List[int] = field(default_factory=list)
    leader_id: int | None = None
    cohesion: float = 1.0


class GroupDynamics:
    def __init__(self) -> None:
        self.groups: List[Group] = []

    def form_group(self, agent_ids: List[int]) -> int:
        leader = agent_ids[0] if agent_ids else None
        gid = len(self.groups)
        self.groups.append(Group(member_ids=agent_ids, leader_id=leader))
        return gid
