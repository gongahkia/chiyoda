const BLOCKED_TOKENS = new Set(["X"]);
const EXIT_TOKEN = "E";
const DEFAULT_WALK_SPEED_CELLS_S = 1.35;

export function browserSimSupport(viewerData, options = {}) {
  const maxAgents = Number(options.maxAgents || 200);
  const floors = normalizeFloors(viewerData.layout_floors || []);
  const firstFrame = (viewerData.frames || [])[0] || { agents: [] };
  if (!floors.length) {
    return { ok: false, reason: "no runtime floors available" };
  }
  if (!Array.isArray(firstFrame.agents) || firstFrame.agents.length === 0) {
    return { ok: false, reason: "no initial replay agents available" };
  }
  if (firstFrame.agents.length > maxAgents) {
    return { ok: false, reason: `agent count exceeds ${maxAgents}` };
  }
  const model = buildModel(viewerData);
  if (!model.exits.length) {
    return { ok: false, reason: "no exit cells in runtime floors" };
  }
  const distance = distanceToExit(model);
  const reachableInitialAgents = firstFrame.agents.some(agent => {
    const floorId = String(agent.floor_id || model.defaultFloor.id);
    const floor = model.floorsById.get(floorId) || model.defaultFloor;
    const cell = initialCell(agent, floor);
    return Number.isFinite(cellDistance(distance, keyOf(floor.id, cell.x, cell.y)));
  });
  if (!reachableInitialAgents) {
    return { ok: false, reason: "no initial agents can reach an exit" };
  }
  return {
    ok: true,
    reason: "supported",
    floors: model.floors.length,
    connectors: model.connectors.length,
  };
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
  const model = buildModel(viewerData);
  const distance = distanceToExit(model);
  const queues = connectorQueues(model.connectors);
  const initialFrame = viewerData.frames[0];
  const agents = initialFrame.agents.slice(0, maxAgents).map(agent => {
    const floorId = String(agent.floor_id || model.defaultFloor.id);
    const floor = model.floorsById.get(floorId) || model.defaultFloor;
    const cell = initialCell(agent, floor);
    return {
      id: Number(agent.id),
      x: cell.x,
      y: cell.y,
      z: floor.z,
      floor_id: floor.id,
      speed: Math.max(DEFAULT_WALK_SPEED_CELLS_S, Number(agent.speed || 0)),
      entropy: Number(agent.entropy || 0),
      state: String(agent.state || "BROWSER"),
      evacuated: false,
      waitingConnectorId: null,
      transfer: null,
    };
  });

  const frames = [];
  const steps = Math.max(1, Math.ceil(durationS / dtS));
  for (let step = 0; step <= steps; step += 1) {
    const timeS = step * dtS;
    finishTransfers(agents, timeS, model);
    serviceConnectorQueues(queues, timeS, model);
    if (step % sampleEvery === 0 || step === steps) {
      frames.push(frameFromAgents(step, timeS, agents, model));
    }
    if (step < steps) {
      stepAgents(agents, model, distance, queues, dtS, timeS);
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
      floor_count: model.floors.length,
      connector_count: model.connectors.length,
      connector_usage: connectorUsage(queues),
      target_steps_per_second: Number(options.targetStepsPerSecond || 10),
    },
  };
}

function stepAgents(agents, model, distance, queues, dtS, timeS) {
  for (const agent of agents) {
    if (agent.evacuated || agent.transfer || agent.waitingConnectorId) continue;
    const cell = agentCell(agent, model);
    if (isExit(model, cell.floor, cell.x, cell.y)) {
      agent.evacuated = true;
      continue;
    }
    const next = nextCell(cell, model, distance);
    if (!next) continue;
    if (next.connector) {
      requestConnectorTransfer(agent, next.connector, cell, next, queues, timeS, model);
      continue;
    }
    const dx = next.x - agent.x;
    const dy = next.y - agent.y;
    const length = Math.hypot(dx, dy);
    const stride = agent.speed * dtS;
    if (length <= stride || length < 1e-9) {
      agent.x = next.x;
      agent.y = next.y;
    } else {
      agent.x += (dx / length) * stride;
      agent.y += (dy / length) * stride;
    }
    const after = agentCell(agent, model);
    if (isExit(model, after.floor, after.x, after.y)) agent.evacuated = true;
  }
}

function requestConnectorTransfer(agent, connector, source, target, queues, timeS, model) {
  const queue = queues.get(connector.id);
  if (!queue) return;
  const record = {
    agent,
    connector,
    source,
    target: { floor: target.floor, x: target.x, y: target.y },
  };
  if (canStartTransfer(queue)) {
    startTransfer(record, queue, timeS, model);
    return;
  }
  if (agent.waitingConnectorId) return;
  agent.waitingConnectorId = connector.id;
  queue.waiting.push(record);
}

function serviceConnectorQueues(queues, timeS, model) {
  for (const queue of queues.values()) {
    queue.active = queue.active.filter(record => record.agent.transfer);
    while (queue.waiting.length && canStartTransfer(queue)) {
      const record = queue.waiting.shift();
      if (record.agent.evacuated || record.agent.transfer) continue;
      record.agent.waitingConnectorId = null;
      startTransfer(record, queue, timeS, model);
    }
  }
}

function startTransfer(record, queue, timeS, model) {
  const durationS = connectorDuration(record.connector, record.source, record.target, model);
  const sourceFloor = model.floorsById.get(record.source.floor) || model.defaultFloor;
  const targetFloor = model.floorsById.get(record.target.floor) || model.defaultFloor;
  record.agent.transfer = {
    connector_id: record.connector.id,
    source: {
      floor: sourceFloor.id,
      x: record.source.x,
      y: record.source.y,
      z: sourceFloor.z,
    },
    target: {
      floor: targetFloor.id,
      x: record.target.x,
      y: record.target.y,
      z: targetFloor.z,
    },
    start_s: timeS,
    arrival_s: timeS + durationS,
  };
  queue.active.push(record);
  queue.started += 1;
}

function finishTransfers(agents, timeS, model) {
  for (const agent of agents) {
    const transfer = agent.transfer;
    if (!transfer) continue;
    if (timeS < transfer.arrival_s) {
      const point = transferPoint(transfer, timeS);
      agent.x = point.x;
      agent.y = point.y;
      agent.z = point.z;
      continue;
    }
    const floor = model.floorsById.get(transfer.target.floor) || model.defaultFloor;
    agent.floor_id = floor.id;
    agent.x = transfer.target.x;
    agent.y = transfer.target.y;
    agent.z = floor.z;
    agent.transfer = null;
    if (isExit(model, floor.id, agent.x, agent.y)) agent.evacuated = true;
  }
}

function transferPoint(transfer, timeS) {
  const span = Math.max(1e-9, transfer.arrival_s - transfer.start_s);
  const t = clamp((timeS - transfer.start_s) / span, 0, 1);
  return {
    x: lerp(transfer.source.x, transfer.target.x, t),
    y: lerp(transfer.source.y, transfer.target.y, t),
    z: lerp(transfer.source.z, transfer.target.z, t),
  };
}

function nextCell(cell, model, distance) {
  const current = cellDistance(distance, keyOf(cell.floor, cell.x, cell.y));
  if (!Number.isFinite(current)) return null;
  let best = { floor: cell.floor, x: cell.x, y: cell.y, distance: current, connector: null };
  for (const candidate of forwardNeighbors(cell, model)) {
    const candidateDistance = cellDistance(distance, keyOf(candidate.floor, candidate.x, candidate.y));
    if (candidateDistance < best.distance) {
      best = { ...candidate, distance: candidateDistance };
    }
  }
  return best.distance < current ? best : null;
}

function distanceToExit(model) {
  const distance = new Map();
  const queue = [];
  for (const exit of model.exits) {
    const key = keyOf(exit.floor, exit.x, exit.y);
    distance.set(key, 0);
    queue.push({ ...exit, distance: 0 });
  }
  while (queue.length) {
    queue.sort((a, b) => a.distance - b.distance);
    const cell = queue.shift();
    const key = keyOf(cell.floor, cell.x, cell.y);
    const base = distance.get(key);
    if (cell.distance > base) continue;
    for (const previous of reverseNeighbors(cell, model)) {
      const nextDistance = base + Number(previous.cost || 1);
      const previousKey = keyOf(previous.floor, previous.x, previous.y);
      if (nextDistance >= cellDistance(distance, previousKey)) continue;
      distance.set(previousKey, nextDistance);
      queue.push({ floor: previous.floor, x: previous.x, y: previous.y, distance: nextDistance });
    }
  }
  return distance;
}

function forwardNeighbors(cell, model) {
  const result = cardinalNeighbors(cell, model);
  for (const edge of model.connectorsBySource.get(keyOf(cell.floor, cell.x, cell.y)) || []) {
    result.push({ ...edge.to, connector: edge.connector });
  }
  return result;
}

function reverseNeighbors(cell, model) {
  const result = cardinalNeighbors(cell, model);
  for (const edge of model.connectorsByTarget.get(keyOf(cell.floor, cell.x, cell.y)) || []) {
    result.push({ ...edge.from, connector: edge.connector, cost: edge.cost });
  }
  return result;
}

function cardinalNeighbors(cell, model) {
  const result = [];
  for (const [dx, dy] of [[1, 0], [-1, 0], [0, 1], [0, -1]]) {
    const x = cell.x + dx;
    const y = cell.y + dy;
    if (isWalkable(model, cell.floor, x, y)) {
      result.push({ floor: cell.floor, x, y, connector: null, cost: 1 });
    }
  }
  return result;
}

function buildModel(viewerData) {
  const floors = normalizeFloors(viewerData.layout_floors || []);
  const defaultFloor = floors[0] || { id: "0", z: 0, grid: [] };
  const floorsById = new Map(floors.map(floor => [floor.id, floor]));
  const connectors = normalizeConnectors(viewerData.layout_connectors || [], floorsById);
  const connectorsBySource = new Map();
  const connectorsByTarget = new Map();
  for (const connector of connectors) {
    addConnectorEdge(connector.from, connector.to, connector, floorsById, connectorsBySource, connectorsByTarget);
    if (connector.bidirectional) {
      addConnectorEdge(connector.to, connector.from, connector, floorsById, connectorsBySource, connectorsByTarget);
    }
  }
  const exits = [];
  for (const floor of floors) {
    for (let y = 0; y < floor.grid.length; y += 1) {
      for (let x = 0; x < floor.grid[y].length; x += 1) {
        if (floor.grid[y][x] === EXIT_TOKEN) exits.push({ floor: floor.id, x, y });
      }
    }
  }
  return { floors, floorsById, defaultFloor, connectors, connectorsBySource, connectorsByTarget, exits };
}

function addConnectorEdge(from, to, connector, floorsById, bySource, byTarget) {
  if (!isWalkableInFloors(floorsById, from.floor, from.x, from.y)) return;
  if (!isWalkableInFloors(floorsById, to.floor, to.x, to.y)) return;
  const edge = {
    from,
    to,
    connector,
    cost: connectorDuration(connector, from, to, { floorsById, defaultFloor: floorsById.values().next().value || { z: 0 } }),
  };
  pushMapList(bySource, keyOf(from.floor, from.x, from.y), edge);
  pushMapList(byTarget, keyOf(to.floor, to.x, to.y), edge);
}

function normalizeFloors(rawFloors) {
  return rawFloors.map((floor, index) => ({
    id: String(floor.id ?? index),
    z: Number(floor.z || 0),
    grid: normalizeGrid(floor.grid || []),
  })).filter(floor => floor.grid.length);
}

function normalizeGrid(rawGrid) {
  return rawGrid.map(row => {
    if (Array.isArray(row)) return row.map(token => String(token || ".").slice(0, 1));
    return String(row).split("").map(token => token || ".");
  });
}

function normalizeConnectors(rawConnectors, floorsById) {
  return rawConnectors.map((connector, index) => ({
    id: String(connector.id ?? `connector_${index + 1}`),
    type: String(connector.type || "stairs"),
    from: normalizeEndpoint(connector.from || connector.from_cell),
    to: normalizeEndpoint(connector.to || connector.to_cell),
    bidirectional: connector.bidirectional !== false,
    width: Number(connector.width ?? 1),
    speed_multiplier: Number(connector.speed_multiplier ?? 1),
    capacity: connector.capacity == null ? defaultCapacity(connector) : Math.max(1, Number(connector.capacity)),
    dwell_s: Number(connector.dwell_s ?? 0),
    travel_s: Number(connector.travel_s ?? 0),
  })).filter(connector =>
    connector.from && connector.to &&
    floorsById.has(connector.from.floor) &&
    floorsById.has(connector.to.floor)
  );
}

function normalizeEndpoint(value) {
  if (!value) return null;
  if (Array.isArray(value) && value.length >= 3) {
    return { floor: String(value[0]), x: Number(value[1]), y: Number(value[2]) };
  }
  return { floor: String(value.floor), x: Number(value.x), y: Number(value.y) };
}

function connectorQueues(connectors) {
  const queues = new Map();
  for (const connector of connectors) {
    queues.set(connector.id, {
      connector,
      capacity: Math.max(1, Math.round(Number(connector.capacity || 1))),
      waiting: [],
      active: [],
      started: 0,
    });
  }
  return queues;
}

function connectorUsage(queues) {
  const usage = {};
  for (const [id, queue] of queues.entries()) usage[id] = queue.started;
  return usage;
}

function canStartTransfer(queue) {
  return queue.active.length < queue.capacity;
}

function connectorDuration(connector, source, target, model) {
  const dwellS = Number(connector.dwell_s || 0);
  const travelS = Number(connector.travel_s || 0);
  if (travelS > 0 || connector.type === "elevator") {
    return Math.max(0.01, dwellS + travelS);
  }
  const sourceFloor = model.floorsById.get(source.floor) || model.defaultFloor;
  const targetFloor = model.floorsById.get(target.floor) || model.defaultFloor;
  const dx = target.x - source.x;
  const dy = target.y - source.y;
  const dz = Number(targetFloor.z || 0) - Number(sourceFloor.z || 0);
  const distance = Math.hypot(dx, dy, dz);
  const speed = DEFAULT_WALK_SPEED_CELLS_S * Math.max(Number(connector.speed_multiplier || 1), 1e-6);
  return Math.max(0.01, dwellS + distance / speed);
}

function defaultCapacity(connector) {
  if (connector.type === "elevator") return 1;
  return Math.max(1, Math.round(Number(connector.width || 1) * 2));
}

function frameFromAgents(step, timeS, agents, model) {
  return {
    step,
    time_s: timeS,
    source: "browser_sim",
    agents: agents
      .filter(agent => !agent.evacuated)
      .map(agent => {
        const point = agent.transfer ? transferPoint(agent.transfer, timeS) : agent;
        const floorId = agent.transfer ? agent.transfer.source.floor : agent.floor_id;
        return {
          id: agent.id,
          x: point.x,
          y: point.y,
          z: point.z,
          floor_id: floorId,
          cell_x: Math.round(point.x),
          cell_y: Math.round(point.y),
          speed: agent.speed,
          entropy: agent.entropy,
          state: agent.transfer ? "TRANSFER" : agent.state,
          intent: "EVACUATE",
        };
      }),
  };
}

function initialCell(agent, floor) {
  const width = floorWidth(floor);
  const height = floor.grid.length;
  const x = Number.isFinite(Number(agent.cell_x)) ? Number(agent.cell_x) : Number(agent.x);
  const y = Number.isFinite(Number(agent.cell_y)) ? Number(agent.cell_y) : Number(agent.y);
  return {
    x: clamp(Math.round(x), 0, Math.max(0, width - 1)),
    y: clamp(Math.round(y), 0, Math.max(0, height - 1)),
  };
}

function agentCell(agent, model) {
  const floor = model.floorsById.get(agent.floor_id) || model.defaultFloor;
  const width = floorWidth(floor);
  return {
    floor: floor.id,
    x: clamp(Math.round(agent.x), 0, Math.max(0, width - 1)),
    y: clamp(Math.round(agent.y), 0, Math.max(0, floor.grid.length - 1)),
  };
}

function floorWidth(floor) {
  return Math.max(0, ...floor.grid.map(row => row.length));
}

function isExit(model, floorId, x, y) {
  return model.floorsById.get(String(floorId))?.grid?.[y]?.[x] === EXIT_TOKEN;
}

function isWalkable(model, floorId, x, y) {
  return isWalkableInFloors(model.floorsById, floorId, x, y);
}

function isWalkableInFloors(floorsById, floorId, x, y) {
  const token = floorsById.get(String(floorId))?.grid?.[y]?.[x];
  return typeof token === "string" && !BLOCKED_TOKENS.has(token);
}

function cellDistance(distance, key) {
  return distance.get(key) ?? Infinity;
}

function keyOf(floor, x, y) {
  return `${floor},${x},${y}`;
}

function pushMapList(map, key, value) {
  const list = map.get(key) || [];
  list.push(value);
  map.set(key, list);
}

function lerp(a, b, t) {
  return a + (b - a) * t;
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
