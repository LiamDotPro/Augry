/**
 * The cloud's process format, `schemaVersion: 1` with lanes of ordered agent
 * and shell steps (Terminal-Grid-Cloud, ProcessDefinitionV1), as a process
 * in the shared format (schemaVersion 2) the editor can draw. Templates come from the
 * server in this form. Each lane becomes a terminal row, its stages run in
 * order, and each step waits for the one before it in its lane.
 */

import { validateProcess } from "./json";
import { PROCESS_FORMAT, type Process, type Stage, type Step, type Surface } from "./types";

const isObj = (v: unknown): v is Record<string, unknown> => !!v && typeof v === "object" && !Array.isArray(v);
const str = (v: unknown): string | undefined => (typeof v === "string" && v.trim() ? v : undefined);

/** Is this the lanes format rather than our own? */
export function isV1(definition: unknown): boolean {
  return isObj(definition) && definition.schemaVersion === 1 && Array.isArray(definition.lanes);
}

/** The V1 agent names as this app's rows name them. */
function agentOf(name: unknown): string {
  return name === "codex" ? "codex" : "claude-code";
}

/** An id not taken yet: `id`, else `prefix-id`, else with a number. */
function fresh(id: string, prefix: string, taken: Set<string>): string {
  let out = taken.has(id) ? `${prefix}-${id}` : id;
  for (let n = 2; taken.has(out); n++) out = `${prefix}-${id}-${n}`;
  taken.add(out);
  return out;
}

export interface V1Meta {
  /** The process id to give it. */
  id: string;
  /** Where it came from, e.g. "template:review-changes@1". */
  from?: string;
}

/** The V1 definition as a process, or null when it isn't a usable one. */
export function processFromV1(definition: unknown, meta: V1Meta): Process | null {
  if (!isV1(definition)) return null;
  const d = definition as Record<string, unknown>;
  const surfaces: Surface[] = [{ id: "you", kind: "you" }];
  const stages: Stage[] = [];
  const steps: Step[] = [];
  const rowIds = new Set(["you"]);
  const stageIds = new Set<string>();
  const stepIds = new Set<string>();

  for (const lane of d.lanes as unknown[]) {
    if (!isObj(lane) || !Array.isArray(lane.stages)) return null;
    const laneSteps = lane.stages.flatMap((stage) => (isObj(stage) && Array.isArray(stage.steps) ? stage.steps : []));
    const firstAgent = laneSteps.find((step) => isObj(step) && step.kind === "agent") as Record<string, unknown> | undefined;
    const row = fresh(str(lane.id) ?? "lane", "lane", rowIds);
    surfaces.push({
      id: row,
      kind: "terminal",
      ...(firstAgent ? { agent: agentOf(firstAgent.agent) } : {}),
      cwd: "repo",
      name: str(lane.title) ?? row,
    });

    let previous: string | undefined;
    for (const stage of lane.stages) {
      if (!isObj(stage) || !Array.isArray(stage.steps)) return null;
      const stageId = fresh(str(stage.id) ?? "stage", row, stageIds);
      stages.push({ id: stageId, name: str(stage.title) ?? stageId });
      for (const step of stage.steps) {
        if (!isObj(step)) return null;
        const id = fresh(str(step.id) ?? "step", row, stepIds);
        const out: Step = { id, name: str(step.title) ?? id, stage: stageId, on: row };
        if (step.kind === "shell") out.run = typeof step.command === "string" ? step.command : "";
        else out.prompt = typeof step.prompt === "string" ? step.prompt : "";
        if (previous) out.after = previous;
        steps.push(out);
        previous = id;
      }
    }
  }

  const process: Process = {
    schemaVersion: PROCESS_FORMAT,
    id: meta.id,
    name: str(d.name) ?? meta.id,
    version: 1,
    ...(meta.from ? { from: meta.from } : {}),
    ...(str(d.description) ? { description: d.description } : {}),
    ...(Array.isArray(d.requires) ? { requires: d.requires } : {}),
    start: { on: "manual" },
    surfaces,
    stages,
    steps,
  };
  return validateProcess(process).length ? null : process;
}
