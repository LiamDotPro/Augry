/**
 * The tab's starting data until the backend reads `.augry/processes/` and
 * the run log: the three processes and their histories, the runs the design
 * shows (Feature delivery run 4 waiting at Review changes, Bug fix run 12 in
 * its loops, Crash report run 3 just started by a file), and the issues the
 * Copy and run sheet offers.
 */

import { BUG_FIX, CRASH_REPORT, featureDeliveryHistory } from "./examples";
import { buildGraph } from "./graph";
import { cloneProcess, hashOf } from "./json";
import { isRepeat } from "./model";
import type { Process, ProcessRecord, ProcessVersion, Run, StepRun, StepState } from "./types";

const MIN = 60000;
const HOUR = 60 * MIN;
const DAY = 24 * HOUR;

function version(process: Process, message: string, at: number, note?: string): ProcessVersion {
  return { version: process.version, message, by: "You", at, hash: hashOf(process), process, note };
}

export function seedRecords(now: number): ProcessRecord[] {
  const [f1, f2, f3, f4, f5, f6, f7] = featureDeliveryHistory();
  const feature: ProcessRecord = {
    id: "feature-delivery",
    draft: null,
    versions: [
      version(f1!, "Created from template Feature delivery@3", now - 9 * DAY),
      version(f2!, "Renamed stages", now - 8 * DAY),
      version(f3!, "Restored v1 checks", now - 7 * DAY - 2 * HOUR, "restored v1"),
      version(f4!, "Shell runs in the worktree", now - 3 * DAY - 3 * HOUR),
      version(f5!, "Approve plan before building", now - 3 * DAY),
      version(f6!, "Smoke test in the browser", now - DAY - 2 * HOUR),
      version(f7!, "Added Summarise changes, tightened checks", now - 12 * MIN),
    ],
  };

  const bug3 = cloneProcess(BUG_FIX);
  const bug2 = { ...cloneProcess(BUG_FIX), version: 2 };
  bug2.steps.find((s) => s.id === "found")!.decide = "true";
  const bug1 = { ...cloneProcess(bug2), version: 1, from: "template:bug-fix@2" };
  delete bug1.loops;
  const bug: ProcessRecord = {
    id: "bug-fix",
    draft: null,
    versions: [
      version(bug1, "Created from template Bug fix@2", now - 12 * DAY),
      version(bug2, "Fix each failing test in turn", now - 10 * DAY),
      version(bug3, "Claude decides if it reproduced", now - 4 * DAY),
    ],
  };

  const crash: ProcessRecord = {
    id: "crash-report",
    draft: null,
    versions: [version({ ...cloneProcess(CRASH_REPORT), from: "process:bug-fix@3" }, "Copied from Bug fix, starts from a watched folder", now - 6 * DAY)],
  };
  return [feature, bug, crash];
}

const states = (map: Record<string, [StepState, string]>): Record<string, StepRun> =>
  Object.fromEntries(Object.entries(map).map(([id, [state, note]]) => [id, { state, note }]));

/** A finished run: every step done, or for a stopped one, done up to where it stopped. */
function finished(process: Process, stoppedAt: string | null): Record<string, StepRun> {
  const graph = buildGraph(process);
  const stopCol = stoppedAt ? graph.col[stoppedAt]! : Infinity;
  const out: Record<string, StepRun> = {};
  for (const s of process.steps) {
    const c = graph.col[s.id]!;
    out[s.id] =
      s.id === stoppedAt
        ? { state: "stopped", note: "stopped" }
        : c < stopCol
          ? { state: "done", note: s.decide ? "yes" : "" }
          : { state: "pending", note: "" };
    // The "ask you" branch is only taken when something went wrong.
    if (s.ask && out[s.id]!.state === "done") out[s.id] = { state: "skipped", note: "skipped" };
  }
  return out;
}

function pastRun(record: ProcessRecord, number: number, version: number, at: number, minutes: number, result: string, trigger: string): Run {
  const process = record.versions.find((v) => v.version === version)!.process;
  const stopped = result.startsWith("Stopped");
  const repeat = process.steps.find((s) => s.decide && isRepeat(s.no));
  const steps = finished(process, stopped ? (repeat?.id ?? null) : null);
  return {
    id: `${record.id}#${number}`, number, processId: record.id, version, hash: hashOf(process), process: cloneProcess(process),
    status: stopped ? "stopped" : "done", startedAt: at, endedAt: at + minutes * MIN, since: at + minutes * MIN,
    trigger, steps, loops: stopped && repeat ? { [repeat.id]: "stopped after 3 tries" } : {}, panes: { claude: 3, codex: 2, shell: 4 }, tries: {}, result,
  };
}

export function seedRuns(records: ProcessRecord[], now: number): Run[] {
  const [feature, bug, crash] = records as [ProcessRecord, ProcessRecord, ProcessRecord];
  const v6 = feature.versions.find((v) => v.version === 6)!;

  const run4: Run = {
    id: "feature-delivery#4", number: 4, processId: feature.id, version: 6, hash: v6.hash, process: cloneProcess(v6.process),
    status: "waiting", startedAt: now - 58 * MIN, since: now - 29 * MIN, trigger: "issue #142",
    detail: "#142",
    steps: states({
      plan: ["done", "4m"], approve: ["done", "10:42"], impl: ["done", "11m"], checks: ["done", "2nd try"], passed: ["done", "yes"],
      smoke: ["done", "4 shots"], review: ["waiting", "waiting"], pr: ["pending", ""],
    }),
    loops: { passed: "ran twice" },
    panes: { claude: 3, codex: 2, shell: 4 },
    tries: { passed: 1 },
    laneNotes: {
      you: { text: "1 step waiting", tone: "warn" },
      claude: { text: "Pane 3 · finished 10:59", tone: "ok" },
      codex: { text: "Pane 2 · finished 10:58", tone: "ok" },
      shell: { text: "Pane 4 · pnpm dev running", tone: "accent" },
      browser: { text: "Chrome · 4 screenshots saved", tone: "ok" },
    },
    waitSummary: "Checks passed on the 2nd try and the smoke test saved 4 screenshots. 6 files changed.",
  };

  const bug3 = bug.versions[bug.versions.length - 1]!;
  const run12: Run = {
    id: "bug-fix#12", number: 12, processId: bug.id, version: 3, hash: bug3.hash, process: cloneProcess(bug3.process),
    status: "running", startedAt: now - 8 * MIN, since: now - 8 * MIN, trigger: "webhook", detail: "TypeError in parseDate",
    steps: states({
      repro: ["done", "6m"], found: ["done", "yes"], info: ["skipped", "skipped"], fix: ["running", "2m"], test: ["pending", ""],
      green: ["pending", ""], verify: ["pending", ""], review: ["pending", ""], pr: ["pending", ""],
    }),
    loops: { green: "try 1 of 3", each: "test 2 of 3" },
    panes: { claude: 3, codex: 2, shell: 4 },
    tries: { each: 2 },
    laneNotes: {
      you: { text: "nothing waiting", tone: "muted" },
      claude: { text: "Pane 3 · finished 14:08", tone: "ok" },
      codex: { text: "Pane 2 · fixing parseDate edge case", tone: "accent" },
      shell: { text: "Pane 4 · idle", tone: "muted" },
      browser: { text: "Chrome · not opened yet", tone: "muted" },
    },
  };

  const crash1 = crash.versions[0]!;
  const run3: Run = {
    id: "crash-report#3", number: 3, processId: crash.id, version: 1, hash: crash1.hash, process: cloneProcess(crash1.process),
    status: "running", startedAt: now - MIN, since: now - MIN, trigger: "folder", detail: "crash-2026-10-11.log",
    steps: states(Object.fromEntries(crash1.process.steps.map((s) => [s.id, [s.id === "repro" ? "running" : "pending", ""]])) as Record<string, [StepState, string]>),
    loops: {},
    panes: { claude: 5, codex: 6, shell: 7 },
    tries: {},
  };

  return [
    run3,
    run12,
    run4,
    pastRun(crash, 2, 1, now - DAY - 5 * HOUR, 41, "PR open", "folder"),
    pastRun(bug, 11, 3, now - DAY - 3 * HOUR, 26, "Stopped after 3 tries", "webhook"),
    pastRun(feature, 3, 5, now - 2 * DAY, 75, "PR open", "issue #131"),
    pastRun(feature, 2, 5, now - 3 * DAY + 2 * HOUR, 64, "PR open", "issue #128"),
    pastRun(feature, 1, 5, now - 3 * DAY + HOUR, 52, "PR open", "issue #124"),
  ];
}

/** Open issues labelled ready, for the Copy and run sheet. */
export const ISSUES = [
  { n: "#142", title: "Settings: font size doesn’t apply to new panes" },
  { n: "#139", title: "Pane glow stays after focus" },
  { n: "#137", title: "Add stash count to git status" },
];

/** The newest file in a watched folder, for Run and "Test with the newest file". */
export function newestFile(now: number): string {
  const d = new Date(now);
  return `crash-${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}.log`;
}
