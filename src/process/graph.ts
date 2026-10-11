/**
 * The order of a process: which steps come before which, the column each step
 * sits in, and the loops. Columns are derived, never stored: a step goes one
 * column right of the furthest step it waits on, so steps that wait on the
 * same thing share a column and run together.
 */

import { isRepeat, list } from "./model";
import type { Process, Step } from "./types";

export interface Link {
  from: string;
  to: string;
  branch?: "yes" | "no";
}

export interface LoopSpan {
  id: string;
  kind: "repeat" | "each";
  /** First and last column the loop repeats. */
  from: number;
  to: number;
  label: string;
  /** Repeat loops: the most tries. */
  max?: number;
  /** For-each loops: the steps it repeats. */
  steps: string[];
}

export interface Graph {
  steps: Step[];
  byId: Record<string, Step>;
  preds: Record<string, string[]>;
  links: Link[];
  col: Record<string, number>;
  nCols: number;
  loops: LoopSpan[];
}

/** Steps on a path from `from` to `to`, both included: what a repeat loop runs again. */
function between(links: Link[], from: string, to: string): string[] {
  const reach = (start: string, forward: boolean) => {
    const seen = new Set<string>([start]);
    const queue = [start];
    while (queue.length) {
      const id = queue.shift()!;
      for (const l of links) {
        const [a, b] = forward ? [l.from, l.to] : [l.to, l.from];
        if (a === id && !seen.has(b)) {
          seen.add(b);
          queue.push(b);
        }
      }
    }
    return seen;
  };
  const down = reach(from, true);
  const up = reach(to, false);
  return [...down].filter((id) => up.has(id));
}

export function buildGraph(process: Process): Graph {
  const steps = process.steps;
  const byId: Record<string, Step> = {};
  for (const step of steps) byId[step.id] = step;
  const preds: Record<string, string[]> = {};
  for (const step of steps) preds[step.id] = [];
  const links: Link[] = [];

  // References to steps that do not exist are dropped here, so a half-typed
  // id in the JSON panel never breaks the drawing.
  for (const step of steps) {
    for (const a of list(step.after)) {
      if (!byId[a] || a === step.id) continue;
      preds[step.id]!.push(a);
      links.push({ from: a, to: step.id });
    }
    if (step.decide) {
      for (const t of list(step.yes)) {
        if (!byId[t] || t === step.id) continue;
        preds[t]!.push(step.id);
        links.push({ from: step.id, to: t, branch: "yes" });
      }
      if (Array.isArray(step.no)) {
        for (const t of step.no) {
          if (!byId[t] || t === step.id) continue;
          preds[t]!.push(step.id);
          links.push({ from: step.id, to: t, branch: "no" });
        }
      }
    }
  }

  const col: Record<string, number> = {};
  const visiting = new Set<string>();
  const visit = (id: string): number => {
    const known = col[id];
    if (known !== undefined) return known;
    // A cycle (only possible from hand-edited JSON) is cut where it closes.
    if (visiting.has(id)) return -1;
    visiting.add(id);
    const ps = preds[id] ?? [];
    const c = ps.length ? Math.max(...ps.map(visit)) + 1 : 0;
    visiting.delete(id);
    col[id] = Math.max(0, c);
    return col[id]!;
  };
  steps.forEach((s) => visit(s.id));
  const nCols = steps.length ? Math.max(...Object.values(col)) + 1 : 1;

  const loops: LoopSpan[] = [];
  for (const step of steps) {
    if (!step.decide || !isRepeat(step.no)) continue;
    const target = byId[step.no.repeatFrom];
    if (!target) continue;
    loops.push({
      id: step.id,
      kind: "repeat",
      from: Math.min(col[target.id]!, col[step.id]!),
      to: col[step.id]!,
      label: `Repeat from ${target.name} until “${step.name.replace("?", "")}”`,
      max: step.no.max,
      steps: between(links, target.id, step.id),
    });
  }
  for (const loop of process.loops ?? []) {
    const ids = (loop.steps ?? []).filter((id) => byId[id]);
    if (!ids.length) continue;
    const cs = ids.map((id) => col[id]!);
    loops.push({
      id: loop.id,
      kind: "each",
      from: Math.min(...cs),
      to: Math.max(...cs),
      label: `For each ${loop.as} in ${loop.forEach}`,
      steps: ids,
    });
  }

  return { steps, byId, preds, links, col, nCols, loops };
}

/** Steps nothing leads into: where the start point connects. */
export function firstSteps(graph: Graph): Step[] {
  return graph.steps.filter((s) => !graph.preds[s.id]!.length);
}
