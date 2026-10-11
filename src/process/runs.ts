/**
 * Runs, simulated in memory until the backend drives real panes. A run pins
 * the process version it started with and moves through it a step at a time:
 * agent and shell steps finish on each tick, steps on "you" stop and wait,
 * a repeat decision says no once before it passes so its loop is visible, and
 * a for-each loop goes round three times. Everything here is pure.
 */

import { buildGraph, type Graph } from "./graph";
import { cloneProcess, hashOf } from "./json";
import { isRepeat, surfaceMeta } from "./model";
import type { Process, Run, RunStatus, StepRun, Tone } from "./types";

/** How many items a for-each loop pretends to have. */
export const EACH_TOTAL = 3;

/** Steps on "you" wait for you, decisions included when you are the one who decides. */
const isYou = (process: Process, stepId: string) => {
  const step = process.steps.find((s) => s.id === stepId);
  const surface = process.surfaces.find((s) => s.id === step?.on);
  return surface?.kind === "you" && (!step?.decide || step.decide === "ask");
};

/** A clock time like 10:31. */
export function clock(ts: number): string {
  const d = new Date(ts);
  return `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
}

/** How long ago, short: "now", "8m", "2h", "3d". */
export function agoShort(ts: number, now: number): string {
  const m = Math.max(0, Math.floor((now - ts) / 60000));
  if (m < 1) return "now";
  if (m < 60) return `${m}m`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h}h`;
  return `${Math.floor(h / 24)}d`;
}

/** When, for lists: a clock time today, "yesterday", or "3 days ago". */
export function whenLabel(ts: number, now: number): string {
  const day = (t: number) => new Date(t).setHours(0, 0, 0, 0);
  const days = Math.round((day(now) - day(ts)) / 86400000);
  if (days <= 0) return clock(ts);
  if (days === 1) return "yesterday";
  if (days < 7) return `${days} days ago`;
  return "last week";
}

/** How long ago, in words, for versions: "12 min ago", "yesterday". */
export function agoWords(ts: number, now: number): string {
  const m = Math.max(0, Math.floor((now - ts) / 60000));
  if (m < 1) return "just now";
  if (m < 60) return `${m} min ago`;
  const h = Math.floor(m / 60);
  if (h < 24) return h === 1 ? "an hour ago" : `${h} hours ago`;
  return whenLabel(ts, now);
}

// Starting -----------------------------------------------------------------------

export interface RunStart {
  /** The record the run belongs to; the pinned JSON's own `id` can be edited to anything. */
  processId: string;
  number: number;
  version: number;
  /** "issue #142", "webhook", "folder". */
  trigger: string;
  /** What started it, if more than the trigger says. */
  detail?: string;
  now: number;
  /** The Terminals panes the run's rows use, by surface id. */
  panes?: Record<string, number>;
}

/** Pane numbers for a run's terminal rows, in row order from pane 2. */
export function assignPanes(process: Process): Record<string, number> {
  const panes: Record<string, number> = {};
  let n = 2;
  for (const s of process.surfaces) if (s.kind === "terminal") panes[s.id] = n++;
  return panes;
}

export function startRun(process: Process, start: RunStart): Run {
  const pinned = cloneProcess(process);
  const steps: Record<string, StepRun> = {};
  for (const s of pinned.steps) steps[s.id] = { state: "pending", note: "" };
  const graph = buildGraph(pinned);
  const run: Run = {
    id: `${start.processId}#${start.number}`,
    number: start.number,
    processId: start.processId,
    version: start.version,
    hash: hashOf(pinned),
    process: pinned,
    status: "running",
    startedAt: start.now,
    since: start.now,
    trigger: start.trigger,
    detail: start.detail,
    steps,
    loops: {},
    panes: start.panes ?? assignPanes(pinned),
    tries: {},
  };
  for (const s of pinned.steps) if (!graph.preds[s.id]!.length) begin(run, graph, s.id);
  return settle(run, start.now);
}

// Moving on ----------------------------------------------------------------------

/** Puts a step to work: running, or waiting when it is on "you". */
function begin(run: Run, graph: Graph, id: string) {
  const waiting = isYou(run.process, id);
  run.steps[id] = { state: waiting ? "waiting" : "running", note: waiting ? "waiting" : "" };
  for (const loop of graph.loops) {
    if (loop.kind === "each" && loop.steps.includes(id) && run.loops[loop.id] === undefined) {
      const as = run.process.loops?.find((l) => l.id === loop.id)?.as ?? "item";
      run.tries[loop.id] = 1;
      run.loops[loop.id] = `${as} 1 of ${EACH_TOTAL}`;
    }
  }
}

/** The branch a decision did not take is skipped; startReady skips what only it led to. */
function skipBranch(run: Run, ids: string[]) {
  for (const id of ids) if (run.steps[id]?.state === "pending") run.steps[id] = { state: "skipped", note: "skipped" };
}

/** Starts every pending step whose steps before it are all through. */
function startReady(run: Run, graph: Graph) {
  let moved = true;
  while (moved) {
    moved = false;
    for (const s of graph.steps) {
      if (run.steps[s.id]?.state !== "pending") continue;
      const preds = graph.preds[s.id]!;
      if (!preds.length) continue;
      const states = preds.map((p) => run.steps[p]?.state);
      if (states.every((x) => x === "skipped")) {
        run.steps[s.id] = { state: "skipped", note: "skipped" };
        moved = true;
      } else if (states.every((x) => x === "done" || x === "skipped")) {
        begin(run, graph, s.id);
        moved = true;
      }
    }
  }
}

function statusOf(run: Run): RunStatus {
  const states = Object.values(run.steps).map((s) => s.state);
  if (states.includes("waiting")) return "waiting";
  if (states.includes("running")) return "running";
  return "done";
}

/** Recomputes the status, and the result once nothing is left to do. */
function settle(run: Run, now: number): Run {
  if (run.status === "stopped") return run;
  const status = statusOf(run);
  if (status !== run.status) run.since = now;
  run.status = status;
  if (run.status === "done") {
    run.endedAt = now;
    const end = (run.process.ends ?? []).find((e) => run.steps[e.after]?.state === "done");
    run.result = end ? end.name : "Finished";
  }
  return run;
}

function copy(run: Run): Run {
  return {
    ...run,
    steps: Object.fromEntries(Object.entries(run.steps).map(([k, v]) => [k, { ...v }])),
    loops: { ...run.loops },
    tries: { ...run.tries },
    laneNotes: undefined,
  };
}

/** Puts every step of a loop back to pending, ready to go round again. */
function rewind(run: Run, ids: string[]) {
  for (const id of ids) run.steps[id] = { state: "pending", note: "" };
}

/** One tick: running steps finish and whatever they unblock starts. */
export function advanceRun(previous: Run, now: number): Run {
  if (previous.status === "done" || previous.status === "stopped") return previous;
  const running = Object.entries(previous.steps).filter(([, s]) => s.state === "running");
  if (!running.length) return previous;
  const run = copy(previous);
  const graph = buildGraph(run.process);

  for (const [id] of running) {
    // A loop that rewound this tick may have reset this step already.
    if (run.steps[id]?.state !== "running") continue;
    const step = graph.byId[id]!;
    const minutes = (id.length % 6) + 1;
    if (step.decide) {
      const no = step.no;
      if (isRepeat(no)) {
        const tries = run.tries[id] ?? 0;
        const loop = graph.loops.find((l) => l.kind === "repeat" && l.id === id);
        if (tries < 1 && no.max > 1 && loop) {
          // The first check fails, so the loop goes round once.
          run.tries[id] = tries + 1;
          rewind(run, loop.steps);
          begin(run, graph, no.repeatFrom);
          run.loops[id] = `try ${tries + 2} of ${no.max}`;
          continue;
        }
        run.loops[id] = tries === 0 ? "passed first time" : tries === 1 ? "ran twice" : `ran ${tries + 1} times`;
      }
      run.steps[id] = { state: "done", note: "yes" };
      if (Array.isArray(no) && no.length) skipBranch(run, no);
    } else {
      run.steps[id] = { state: "done", note: `${minutes}m` };
    }

    // The last step of a for-each loop sends it round again until every item is done.
    for (const loop of graph.loops) {
      if (loop.kind !== "each") continue;
      const last = loop.steps.reduce((a, b) => (graph.col[b]! > graph.col[a]! ? b : a));
      if (last !== id) continue;
      const turn = run.tries[loop.id] ?? 1;
      const as = run.process.loops?.find((l) => l.id === loop.id)?.as ?? "item";
      if (turn < EACH_TOTAL) {
        run.tries[loop.id] = turn + 1;
        run.loops[loop.id] = `${as} ${turn + 1} of ${EACH_TOTAL}`;
        rewind(run, loop.steps);
        const first = loop.steps.reduce((a, b) => (graph.col[b]! < graph.col[a]! ? b : a));
        begin(run, graph, first);
      } else {
        run.loops[loop.id] = `all ${EACH_TOTAL} done`;
      }
    }
  }
  startReady(run, graph);
  return settle(run, now);
}

/** You approved, reviewed or answered the waiting step. */
export function approveRun(previous: Run, stepId: string, now: number): Run {
  if (previous.steps[stepId]?.state !== "waiting") return previous;
  const run = copy(previous);
  const graph = buildGraph(run.process);
  const step = graph.byId[stepId];
  if (step?.decide) {
    // Your decision said yes: the no branch is not taken.
    run.steps[stepId] = { state: "done", note: "yes" };
    if (Array.isArray(step.no)) skipBranch(run, step.no);
  } else {
    run.steps[stepId] = { state: "done", note: clock(now) };
  }
  startReady(run, graph);
  return settle(run, now);
}

/** You skipped a question: the run goes on without an answer. */
export function skipRun(previous: Run, stepId: string, now: number): Run {
  if (previous.steps[stepId]?.state !== "waiting") return previous;
  const run = copy(previous);
  run.steps[stepId] = { state: "done", note: "skipped" };
  startReady(run, buildGraph(run.process));
  return settle(run, now);
}

/**
 * Your decision said no: a repeat goes round again until its limit, and a
 * branching no skips the steps it did not lead to.
 */
export function decideNoRun(previous: Run, stepId: string, now: number): Run {
  if (previous.steps[stepId]?.state !== "waiting") return previous;
  const run = copy(previous);
  const graph = buildGraph(run.process);
  const step = graph.byId[stepId];
  if (!step?.decide) return previous;
  const no = step.no;
  if (isRepeat(no)) {
    const tries = (run.tries[stepId] ?? 0) + 1;
    const loop = graph.loops.find((l) => l.kind === "repeat" && l.id === stepId);
    if (!loop || tries >= no.max) {
      const stopped = stopRun(run, now);
      stopped.result = `Stopped after ${no.max} tries`;
      return stopped;
    }
    run.tries[stepId] = tries;
    rewind(run, loop.steps);
    begin(run, graph, no.repeatFrom);
    run.loops[stepId] = `try ${tries + 1} of ${no.max}`;
    return settle(run, now);
  }
  run.steps[stepId] = { state: "done", note: "no" };
  const taken = Array.isArray(no) ? no : [];
  const others = graph.steps.filter((s) => graph.preds[s.id]!.includes(stepId) && !taken.includes(s.id)).map((s) => s.id);
  skipBranch(run, others);
  startReady(run, graph);
  return settle(run, now);
}

/** Send back: the steps before the waiting one run again with your note. */
export function sendBackRun(previous: Run, stepId: string, now: number): Run {
  if (previous.steps[stepId]?.state !== "waiting") return previous;
  const run = copy(previous);
  const graph = buildGraph(run.process);
  run.steps[stepId] = { state: "pending", note: "" };
  let again = graph.preds[stepId]!;
  // Sending back past a decision re-runs the work it decided on.
  again = again.flatMap((p) => (graph.byId[p]?.decide ? graph.preds[p]! : [p]));
  for (const p of graph.preds[stepId]!) if (graph.byId[p]?.decide) run.steps[p] = { state: "pending", note: "" };
  for (const p of again) run.steps[p] = { state: "running", note: "sent back" };
  if (!again.length) begin(run, graph, stepId);
  return settle(run, now);
}

export function stopRun(previous: Run, now: number): Run {
  if (previous.status === "done" || previous.status === "stopped") return previous;
  const run = copy(previous);
  for (const [id, s] of Object.entries(run.steps)) {
    if (s.state === "running" || s.state === "waiting") run.steps[id] = { state: "stopped", note: "stopped" };
  }
  run.status = "stopped";
  run.since = now;
  run.endedAt = now;
  run.result = "Stopped by you";
  return run;
}

// Reading a run ------------------------------------------------------------------

export function waitingStep(run: Run): string | null {
  return Object.entries(run.steps).find(([, s]) => s.state === "waiting")?.[0] ?? null;
}

export function runningStep(run: Run): string | null {
  return Object.entries(run.steps).find(([, s]) => s.state === "running")?.[0] ?? null;
}

/** What a row is doing, for the lane heads in a run. */
export function laneStatus(run: Run): Record<string, { text: string; tone: Tone; pane: number | null }> {
  const out: Record<string, { text: string; tone: Tone; pane: number | null }> = {};
  for (const surface of run.process.surfaces) {
    const pane = run.panes[surface.id] ?? null;
    const note = run.laneNotes?.[surface.id];
    if (note) {
      out[surface.id] = { ...note, pane: surface.kind === "you" ? null : pane };
      continue;
    }
    const where = surface.kind === "browser" ? "Chrome" : `Pane ${pane ?? "?"}`;
    const mine = run.process.steps.filter((s) => s.on === surface.id);
    const states = mine.map((s) => ({ step: s, run: run.steps[s.id] }));
    if (surface.kind === "you") {
      const n = states.filter((s) => s.run?.state === "waiting").length;
      out[surface.id] = n
        ? { text: `${n} step${n === 1 ? "" : "s"} waiting`, tone: "warn", pane: null }
        : { text: "nothing waiting", tone: "muted", pane: null };
      continue;
    }
    const live = states.find((s) => s.run?.state === "running");
    const done = states.filter((s) => s.run?.state === "done");
    if (run.status === "stopped") out[surface.id] = { text: `${where} · stopped`, tone: "muted", pane };
    else if (live) out[surface.id] = { text: `${where} · ${live.step.name.toLowerCase()}`, tone: "accent", pane };
    else if (done.length) out[surface.id] = { text: `${where} · finished`, tone: "ok", pane };
    else out[surface.id] = { text: surface.kind === "browser" ? "Chrome · not opened yet" : `${where} · idle`, tone: "muted", pane };
  }
  return out;
}

/** One line about where a run is up to, for the Runs column. */
export function runHeadline(run: Run): string {
  const by = Object.fromEntries(run.process.steps.map((s) => [s.id, s]));
  const wait = waitingStep(run);
  if (run.status === "waiting" && wait) return `Waiting for you at ${by[wait]!.name}`;
  const live = runningStep(run);
  if ((run.status === "running" || run.status === "waiting") && live) {
    const step = by[live]!;
    // A for-each count ("test 2 of 3") says more than a retry count.
    const loops = buildGraph(run.process).loops.filter((l) => l.steps.includes(live) && /of/.test(run.loops[l.id] ?? ""));
    const loop = loops.find((l) => l.kind === "each") ?? loops[0];
    const where = run.process.surfaces.find((s) => s.id === step.on);
    const pane = where?.kind === "terminal" && run.panes[step.on] ? ` · Pane ${run.panes[step.on]}` : where?.kind === "browser" ? " · Chrome" : "";
    return `${step.name}${pane}${loop ? ` · ${run.loops[loop.id]}` : ` · from ${run.detail ?? run.trigger}`}`;
  }
  return run.result ?? "Finished";
}

/** The waiting step's buttons: the first opens what you need to look at. */
export type WaitBack = "send-back" | "skip" | "decide-no";

export function waitActions(
  run: Run,
  stepId: string,
): { primary: string; approve: string; back: string; backAction: WaitBack } {
  const step = run.process.steps.find((s) => s.id === stepId);
  if (step?.decide) return { primary: "Open run", approve: "Yes", back: "No", backAction: "decide-no" };
  if (step?.ask) return { primary: "Open pane", approve: "Answered", back: "Skip", backAction: "skip" };
  if (step?.approve) return { primary: `Open ${step.approve}`, approve: "Approve", back: "Send back", backAction: "send-back" };
  return { primary: "Open review", approve: "Approve", back: "Send back", backAction: "send-back" };
}

/** What the waiting card says when the run did not come with a summary. */
export function waitSummary(run: Run, stepId: string): string {
  if (run.waitSummary) return run.waitSummary;
  const graph = buildGraph(run.process);
  const before = graph.preds[stepId]!.map((p) => graph.byId[p]!.name);
  const step = graph.byId[stepId]!;
  const what = step.approve ? `Approve ${step.approve} to go on.` : step.ask ? step.ask : `Review ${step.review ?? "the changes"} to go on.`;
  return before.length ? `${before.join(" and ")} finished. ${what}` : what;
}

/** The pane to open for a step: its own, or the pane of the work it waits on. */
export function paneFor(run: Run, stepId: string): number | null {
  const graph = buildGraph(run.process);
  const own = run.panes[graph.byId[stepId]?.on ?? ""];
  if (own) return own;
  for (const p of graph.preds[stepId] ?? []) {
    const pane = run.panes[graph.byId[p]?.on ?? ""];
    if (pane) return pane;
    const further = paneFor(run, p);
    if (further) return further;
  }
  return Object.values(run.panes)[0] ?? null;
}

/** "Claude Code 2 steps" chips for the Copy and run sheet. */
export function surfaceSummary(process: Process): { id: string; icon: string; tone: string; name: string; count: string }[] {
  return process.surfaces
    .filter((s) => s.kind !== "you")
    .map((s) => {
      const meta = surfaceMeta(s);
      const n = process.steps.filter((x) => x.on === s.id && !x.decide).length;
      return { id: s.id, icon: meta.icon, tone: meta.tone, name: meta.name, count: `${n} step${n === 1 ? "" : "s"}`, n };
    })
    .filter((s) => s.n > 0);
}

/** The steps a run stops at for you. */
export function stopsForYou(process: Process): string[] {
  return process.steps.filter((s) => isYou(process, s.id)).map((s) => s.name);
}
