"""
Information entropy metrics for the ITED framework.

Computes Shannon entropy of agent belief states to quantify
uncertainty and measure information dissemination effectiveness.
"""

from __future__ import annotations

from typing import List, Tuple

import numpy as np

from chiyoda.information.field import BeliefVector


def _binary_entropy(p: float) -> float:
    """Shannon entropy of a single Bernoulli variable."""
    if p <= 0.0 or p >= 1.0:
        return 0.0
    return -(p * np.log2(p) + (1.0 - p) * np.log2(1.0 - p))


def agent_entropy(
    beliefs: BeliefVector,
    total_exits: int,
    total_hazards: int = 0,
) -> float:
    """
    Compute information entropy for a single agent's beliefs.

    H = sum of binary entropies for each exit (known/unknown)
      + entropy for hazard estimation accuracy
      + entropy for general danger level

    Normalized to [0, 1] where 0 = perfect knowledge, 1 = maximum uncertainty.
    """
    if total_exits == 0:
        return 0.0

    h_exits = 0.0
    for exit_pos, eb in beliefs.exit_beliefs.items():
        h_exits += _binary_entropy(eb.exists_prob)

    # exits the agent doesn't know about at all contribute maximum entropy
    unknown_exits = total_exits - len(beliefs.exit_beliefs)
    h_exits += unknown_exits * 1.0  # max binary entropy = 1 bit per unknown exit

    # hazard entropy: uncertainty in severity estimation
    h_hazards = 0.0
    if total_hazards > 0:
        known_hazards = len(beliefs.hazard_beliefs)
        # unknown hazards contribute maximum entropy
        h_hazards += max(0, total_hazards - known_hazards) * 1.0
        for hb in beliefs.hazard_beliefs:
            # freshness increases entropy (stale info is less certain)
            h_hazards += _binary_entropy(0.5 + 0.5 * (1.0 - hb.freshness) * (2.0 * hb.severity_est - 1.0))

    # general danger level entropy
    h_danger = _binary_entropy(beliefs.general_danger_level)

    # normalize
    max_entropy = total_exits + total_hazards + 1.0
    raw = h_exits + h_hazards + h_danger
    return min(1.0, raw / max_entropy) if max_entropy > 0 else 0.0


def global_entropy(
    all_beliefs: List[BeliefVector],
    total_exits: int,
    total_hazards: int = 0,
) -> float:
    """Mean information entropy across all agents."""
    if not all_beliefs:
        return 0.0
    entropies = [agent_entropy(b, total_exits, total_hazards) for b in all_beliefs]
    return float(np.mean(entropies))


def entropy_variance(
    all_beliefs: List[BeliefVector],
    total_exits: int,
    total_hazards: int = 0,
) -> float:
    """Variance of entropy — high variance means information inequality."""
    if len(all_beliefs) < 2:
        return 0.0
    entropies = [agent_entropy(b, total_exits, total_hazards) for b in all_beliefs]
    return float(np.var(entropies))


def belief_accuracy(
    beliefs: BeliefVector,
    true_exits: List[Tuple[int, int]],
    true_hazards: list,
) -> float:
    """
    Measure how close an agent's beliefs are to ground truth.

    Returns accuracy in [0, 1] where 1 = perfect knowledge.
    """
    if not true_exits and not true_hazards:
        return 1.0

    score = 0.0
    total = 0.0

    # exit accuracy
    for exit_pos in true_exits:
        total += 1.0
        eb = beliefs.exit_beliefs.get(exit_pos)
        if eb is not None:
            score += eb.exists_prob # higher prob = more accurate
        # else: agent doesn't know → 0 contribution

    # false exits (agent believes exits exist that don't)
    for exit_pos, eb in beliefs.exit_beliefs.items():
        if exit_pos not in true_exits and eb.exists_prob > 0.5:
            total += 1.0
            score += 0.0 # penalize false beliefs

    # hazard accuracy
    for hazard in true_hazards:
        h_pos = (float(hazard.pos[0]), float(hazard.pos[1]))
        total += 1.0
        # find closest matching belief
        best_match = 0.0
        for hb in beliefs.hazard_beliefs:
            dist = np.sqrt(
                (hb.position[0] - h_pos[0]) ** 2 + (hb.position[1] - h_pos[1]) ** 2
            )
            if dist < 3.0:
                # severity accuracy
                sev_acc = 1.0 - abs(hb.severity_est - float(hazard.severity))
                best_match = max(best_match, sev_acc)
        score += best_match

    return score / total if total > 0 else 1.0


def information_efficiency(
    initial_beliefs: List[BeliefVector],
    final_beliefs: List[BeliefVector],
    total_exits: int,
    total_hazards: int = 0,
) -> float:
    """
    Information efficiency: ratio of initial to final entropy.

    η = 1 - H_final / H_initial

    High η means information was effectively distributed during evacuation.
    """
    h_initial = global_entropy(initial_beliefs, total_exits, total_hazards)
    h_final = global_entropy(final_beliefs, total_exits, total_hazards)

    if h_initial < 1e-9:
        return 1.0 # started with perfect info
    return max(0.0, 1.0 - h_final / h_initial)
