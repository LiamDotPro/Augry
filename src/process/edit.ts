/**
 * Edits to a process, as pure functions over the JSON object. The editor's
 * side panel, Add step and Delete all go through here, so the lanes, the
 * runbook and the JSON panel only ever see a whole, consistent process.
 */

import { buildGraph } from "./graph";
import { cloneProcess } from "./json";
import { isRepeat, list } from "./model";
import type { Process, ProcessStart, Step, Surface } from "./types";

/** `after` with one id is written as a string, with none it is left out. */
function setAfter(step: Step, ids: string[]) {
  const unique = [...new Set(ids)];
  if (!unique.length) delete step.after;
  else step.after = unique.length === 1 ? unique[0] : unique;
}

/** Merges fields into a step. A field set to undefined is removed. */
export function updateStep(process: Process, id: string, patch: Partial<Step>): Process {
  const next = cloneProcess(process);
  const step = next.steps.find((s) => s.id === id);
  if (!step) return process;
  for (const [key, value] of Object.entries(patch)) {
    if (value === undefined) delete step[key];
    else step[key] = value;
  }
  return next;
}

export function updateStart(process: Process, start: ProcessStart): Process {
  return { ...cloneProcess(process), start: { ...start } };
}

export function renameStage(process: Process, id: string, name: string): Process {
  const next = cloneProcess(process);
  const stage = next.stages.find((g) => g.id === id);
  if (stage) stage.name = name;
  return next;
}

export function renameProcess(process: Process, name: string): Process {
  return { ...cloneProcess(process), name };
}

function freshId(process: Process, base: string): string {
  const taken = new Set(process.steps.map((s) => s.id));
  if (!taken.has(base)) return base;
  let n = 2;
  while (taken.has(`${base}-${n}`)) n++;
  return `${base}-${n}`;
}

/** What Add step offers. */
export type StepKind = "claude" | "codex" | "shell" | "browser" | "approve" | "review" | "ask" | "decision";

export const STEP_KINDS: { kind: StepKind; name: string; detail: string; icon: string; surface: string }[] = [
  { kind: "claude", name: "Claude Code task", detail: "An agent works in a pane", icon: "claude", surface: "claude" },
  { kind: "codex", name: "Codex task", detail: "An agent works in a pane", icon: "openai", surface: "codex" },
  { kind: "shell", name: "Shell command", detail: "Runs a command and reads its exit code", icon: "term", surface: "shell" },
  { kind: "browser", name: "Browser check", detail: "Opens a page in controlled Chrome", icon: "globe", surface: "browser" },
  { kind: "approve", name: "Approve", detail: "Stops until you approve a file", icon: "gate", surface: "you" },
  { kind: "review", name: "Review changes", detail: "Stops until you review the diff", icon: "review", surface: "you" },
  { kind: "ask", name: "Ask me", detail: "Stops until you answer a question", icon: "question", surface: "you" },
  { kind: "decision", name: "Decision", detail: "Branches on a rule, an agent or you", icon: "decide", surface: "shell" },
];

function newStep(kind: StepKind, id: string, stage: string, process: Process): Step {
  const info = STEP_KINDS.find((k) => k.kind === kind)!;
  // A row the process does not have falls back to its first terminal.
  const on = process.surfaces.some((s) => s.id === info.surface)
    ? info.surface
    : (process.surfaces.find((s) => s.kind === "terminal")?.id ?? process.surfaces[0]?.id ?? info.surface);
  const step: Step = { id, name: info.name, stage, on };
  switch (kind) {
    case "claude":
    case "codex":
      step.name = "New task";
      step.prompt = "Describe the task";
      break;
    case "shell":
      step.name = "Run command";
      step.run = "pnpm test";
      break;
    case "browser":
      step.name = "Open page";
      step.open = "localhost:5173";
      break;
    case "approve":
      step.approve = "plan.md";
      break;
    case "review":
      step.review = "diff";
      break;
    case "ask":
      step.name = "Answer a question";
      step.ask = "What should happen next?";
      break;
    case "decision":
      step.name = "Looks right?";
      step.decide = "true";
      step.yes = [];
      step.no = [];
      break;
  }
  return step;
}

/** The step a new one goes after when nothing is selected: the one the process ends on. */
function lastStep(process: Process): string | null {
  const endAfter = process.ends?.[0]?.after;
  if (endAfter && process.steps.some((s) => s.id === endAfter)) return endAfter;
  if (!process.steps.length) return null;
  const graph = buildGraph(process);
  return [...process.steps].sort((a, b) => graph.col[b.id]! - graph.col[a.id]!)[0]!.id;
}

/**
 * Adds a step after `afterId` (or at the end), between it and whatever came
 * next: the steps that waited on it now wait on the new one, and an end after
 * it moves along. After a decision, the new step goes on its "yes" side.
 */
export function insertStep(process: Process, kind: StepKind, afterId: string | null): { process: Process; id: string } {
  const next = cloneProcess(process);
  const anchorId = afterId && next.steps.some((s) => s.id === afterId) ? afterId : lastStep(next);
  const anchor = anchorId ? next.steps.find((s) => s.id === anchorId)! : null;
  const stage = anchor?.stage ?? next.stages[next.stages.length - 1]?.id ?? "stage-1";
  if (!next.stages.some((g) => g.id === stage)) next.stages.push({ id: stage, name: "Stage 1" });
  const id = freshId(next, kind === "decision" ? "decide" : kind);
  const step = newStep(kind, id, stage, next);

  if (anchor) {
    const successors = next.steps.filter((s) => list(s.after).includes(anchor.id));
    if (anchor.decide) {
      // Between the decision and its "yes" steps.
      const yes = list(anchor.yes);
      anchor.yes = [id];
      for (const t of yes) {
        const target = next.steps.find((s) => s.id === t);
        if (target) setAfter(target, [...list(target.after), id]);
      }
    } else {
      step.after = anchor.id;
      if (step.decide) {
        // The steps that came next now hang off the new decision's "yes".
        step.yes = successors.map((s) => s.id);
        for (const s of successors) setAfter(s, list(s.after).filter((a) => a !== anchor.id));
      } else {
        for (const s of successors) setAfter(s, list(s.after).map((a) => (a === anchor.id ? id : a)));
      }
    }
    for (const end of next.ends ?? []) if (end.after === anchor.id && !anchor.decide) end.after = id;
    const at = next.steps.indexOf(anchor);
    next.steps.splice(at + 1, 0, step);
  } else {
    next.steps.push(step);
  }
  return { process: next, id };
}

/**
 * Removes a step and closes the gap: what waited on it waits on what it
 * waited on, decisions forget it, a loop back to it is dropped, and an end
 * after it moves to the step before.
 */
export function removeStep(process: Process, id: string): Process {
  const next = cloneProcess(process);
  const gone = next.steps.find((s) => s.id === id);
  if (!gone) return process;
  const before = list(gone.after);
  // Decisions that lead to it by "yes" or a "no" list.
  const viaYes = next.steps.filter((s) => s.decide && list(s.yes).includes(id));
  const viaNo = next.steps.filter((s) => s.decide && Array.isArray(s.no) && s.no.includes(id));
  // What it led to itself, as a decision.
  const ownYes = gone.decide ? list(gone.yes) : [];
  const ownNo = gone.decide && Array.isArray(gone.no) ? gone.no : [];

  next.steps = next.steps.filter((s) => s.id !== id);
  for (const s of next.steps) {
    const after = list(s.after);
    if (after.includes(id)) {
      setAfter(s, [...after.filter((a) => a !== id), ...before]);
      for (const d of viaYes) d.yes = [...new Set([...list(d.yes), s.id])];
      for (const d of viaNo) if (Array.isArray(d.no)) d.no = [...new Set([...d.no, s.id])];
    }
    if (ownYes.includes(s.id) || ownNo.includes(s.id)) setAfter(s, [...list(s.after), ...before]);
    if (s.decide) {
      s.yes = list(s.yes).filter((t) => t !== id);
      if (Array.isArray(s.no)) s.no = s.no.filter((t) => t !== id);
      else if (isRepeat(s.no) && s.no.repeatFrom === id) s.no = [];
    }
  }
  if (next.loops) {
    next.loops = next.loops.map((l) => ({ ...l, steps: l.steps.filter((x) => x !== id) })).filter((l) => l.steps.length);
    if (!next.loops.length) delete next.loops;
  }
  if (next.ends) {
    const fallback = before[0] ?? viaYes[0]?.id ?? viaNo[0]?.id;
    next.ends = next.ends.flatMap((e) => (e.after !== id ? [e] : fallback ? [{ ...e, after: fallback }] : []));
  }
  return next;
}

/** Adds a row: another terminal or browser. */
export function addSurface(process: Process, kind: "terminal" | "browser"): Process {
  const next = cloneProcess(process);
  const base = kind === "browser" ? "browser" : "shell";
  const taken = new Set(next.surfaces.map((s) => s.id));
  let n = 2;
  while (taken.has(`${base}-${n}`)) n++;
  const surface: Surface =
    kind === "browser"
      ? { id: `${base}-${n}`, kind, profile: "controlled", name: `Browser ${n}` }
      : { id: `${base}-${n}`, kind, cwd: "worktree", name: `Shell ${n}` };
  next.surfaces.push(surface);
  return next;
}
