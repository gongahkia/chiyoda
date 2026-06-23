const BLOCKED_TOKENS = new Set(["X"]);
const DEFAULT_WALK_SPEED_CELLS_S = 1.35;

export function browserSimSupport(viewerData, options = {}) {
  const maxAgents = Number(options.maxAgents || 200);
  const floors = Array.isArray(viewerData.layout_floors) ? viewerData.layout_floors : [];
  const firstFrame = (viewerData.frames || [])[0] || { agents: [] };
  if (floors.length !== 1) {
    return { ok: false, reason: "single-floor browser simulation only" };
  }
  if (!Array.isArray(firstFrame.agents) || firstFrame.agents.length === 0) {
    return { ok: false, reason: "no initial replay agents available" };
  }
  if (firstFrame.agents.length > maxAgents) {
    return { ok: false, reason: `agent count exceeds ${maxAgents}` };
  }
  const exits = exitCells(floors[0].grid || []);
  if (exits.length === 0) {
    return { ok: false, reason: "no exit cells in active floor" };
  }
  return { ok: true, reason: "supported" };
}

export function runBrowserSimulation(viewerData, options = {}) {
  const startedAt = nowMs();
  const support = browserSimSupport(viewerData, options);
  if (!support.ok) {
    return { ok: false, reason: support.reason, frames: [], summary: {} };
  }

  const durationS = Number(options.durationS || 60);
  const dtS = Number(options.dtS || 0.1);
  const sampleEvery = Math.max(1, Math.round(Number(options.sampleEvery || 5)));
  const maxAgents = Number(options.maxAgents || 200);
  const floor = viewerData.layout_floors[0];
  const grid = floor.grid || [];
  const distance = distanceToExit(grid);
  const initialFrame = viewerData.frames[0];
  const agents = initialFrame.agents.slice(0, maxAgents).map(agent => ({
    id: Number(agent.id),
    x: Number.isFinite(Number(agent.cell_x)) ? Number(agent.cell_x) : Number(agent.x),
    y: Number.isFinite(Number(agent.cell_y)) ? Number(agent.cell_y) : Number(agent.y),
    z: Number(agent.z || floor.z || 0),
    floor_id: String(agent.floor_id || floor.id || "0"),
    speed: Math.max(DEFAULT_WALK_SPEED_CELLS_S, Number(agent.speed || 0)),
    entropy: Number(agent.entropy || 0),
    state: String(agent.state || "BROWSER"),
    evacuated: false
  }));

  const frames = [];
  const steps = Math.max(1, Math.ceil(durationS / dtS));
  for (let step = 0; step <= steps; step += 1) {
    if (step % sampleEvery === 0 || step === steps) {
      frames.push(frameFromAgents(step, step * dtS, agents));
    }
    if (step < steps) {
      stepAgents(agents, grid, distance, dtS);
    }
  }

  const elapsedMs = Math.max(0.001, nowMs() - startedAt);
  const evacuated = agents.filter(agent => agent.evacuated).length;
  const simStepsPerSecond = steps / (elapsedMs / 1000);
  return {
    ok: true,
    reason: "completed",
    frames,
    summary: {
      duration_s: durationS,
      dt_s: dtS,
      sim_steps: steps,
      elapsed_ms: elapsedMs,
      sim_steps_per_second: simStepsPerSecond,
      initial_agents: agents.length,
      evacuated,
      remaining: agents.length - evacuated,
      target_steps_per_second: Number(options.targetStepsPerSecond || 10)
    }
  };
}

function stepAgents(agents, grid, distance, dtS) {
  for (const agent of agents) {
    if (agent.evacuated) continue;
    const cell = agentCell(agent, grid);
    if (isExit(grid, cell.x, cell.y)) {
      agent.evacuated = true;
      continue;
    }
    const next = nextCell(cell.x, cell.y, grid, distance);
    if (!next) continue;
    const targetX = next.x;
    const targetY = next.y;
    const dx = targetX - agent.x;
    const dy = targetY - agent.y;
    const length = Math.hypot(dx, dy);
    const stride = agent.speed * dtS;
    if (length <= stride || length < 1e-9) {
      agent.x = targetX;
      agent.y = targetY;
    } else {
      agent.x += (dx / length) * stride;
      agent.y += (dy / length) * stride;
    }
    const after = agentCell(agent, grid);
    if (isExit(grid, after.x, after.y)) agent.evacuated = true;
  }
}

function nextCell(x, y, grid, distance) {
  const current = cellDistance(distance, x, y);
  if (!Number.isFinite(current)) return null;
  let best = { x, y, distance: current };
  for (const [dx, dy] of [[1, 0], [-1, 0], [0, 1], [0, -1]]) {
    const nx = x + dx;
    const ny = y + dy;
    const candidate = cellDistance(distance, nx, ny);
    if (candidate < best.distance) {
      best = { x: nx, y: ny, distance: candidate };
    }
  }
  return best.distance < current ? best : null;
}

function distanceToExit(grid) {
  const height = grid.length;
  const width = Math.max(0, ...grid.map(row => row.length));
  const distance = Array.from({ length: height }, () =>
    Array.from({ length: width }, () => Infinity)
  );
  const queue = [];
  for (const exit of exitCells(grid)) {
    distance[exit.y][exit.x] = 0;
    queue.push(exit);
  }
  for (let index = 0; index < queue.length; index += 1) {
    const cell = queue[index];
    const base = distance[cell.y][cell.x];
    for (const [dx, dy] of [[1, 0], [-1, 0], [0, 1], [0, -1]]) {
      const nx = cell.x + dx;
      const ny = cell.y + dy;
      if (!isWalkable(grid, nx, ny)) continue;
      if (distance[ny][nx] <= base + 1) continue;
      distance[ny][nx] = base + 1;
      queue.push({ x: nx, y: ny });
    }
  }
  return distance;
}

function frameFromAgents(step, timeS, agents) {
  return {
    step,
    time_s: timeS,
    source: "browser_sim",
    agents: agents
      .filter(agent => !agent.evacuated)
      .map(agent => ({
        id: agent.id,
        x: agent.x,
        y: agent.y,
        z: agent.z,
        floor_id: agent.floor_id,
        speed: agent.speed,
        entropy: agent.entropy,
        state: agent.state,
        intent: "EVACUATE"
      }))
  };
}

function agentCell(agent, grid) {
  const height = grid.length;
  const width = Math.max(0, ...grid.map(row => row.length));
  return {
    x: clamp(Math.round(agent.x), 0, Math.max(0, width - 1)),
    y: clamp(Math.round(agent.y), 0, Math.max(0, height - 1))
  };
}

function exitCells(grid) {
  const cells = [];
  for (let y = 0; y < grid.length; y += 1) {
    for (let x = 0; x < grid[y].length; x += 1) {
      if (grid[y][x] === "E") cells.push({ x, y });
    }
  }
  return cells;
}

function isExit(grid, x, y) {
  return grid[y]?.[x] === "E";
}

function isWalkable(grid, x, y) {
  const token = grid[y]?.[x];
  return typeof token === "string" && !BLOCKED_TOKENS.has(token);
}

function cellDistance(distance, x, y) {
  return distance[y]?.[x] ?? Infinity;
}

function clamp(value, min, max) {
  return Math.max(min, Math.min(max, value));
}

function nowMs() {
  if (typeof performance !== "undefined" && performance.now) {
    return performance.now();
  }
  return Date.now();
}
