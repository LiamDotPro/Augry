/**
 * The `augry/process@1` file: one JSON file per process, at
 * `.augry/processes/<id>.json`. Every Process tab view (lanes, runbook, the
 * JSON panel, version diffs) is drawn from this one object, so the types
 * follow the file exactly. Unknown keys are kept: the side panel writes a few
 * optional settings (doneWhen, notify…) straight into the step.
 */

export const PROCESS_SCHEMA = "augry/process@1";

/** What starts a run. `as` names the values steps can use (ticket.title, file.path). */
export type StartKind = "ticket" | "webhook" | "manual" | "schedule" | "branch" | "notes" | "folder";

export interface ProcessStart {
  on: StartKind;
  as?: string;
  /** ticket */
  source?: string;
  query?: string;
  /** webhook and folder */
  path?: string;
  from?: string;
  reach?: "local" | "relay";
  /** folder */
  match?: string;
  when?: "added" | "changed";
  /** schedule */
  cron?: string;
  /** branch (git) */
  event?: string;
  [key: string]: unknown;
}

/** A row of the lanes: something on this computer that does the work. */
export interface Surface {
  id: string;
  kind: "you" | "terminal" | "browser";
  /** A terminal row that runs an agent ("claude-code", "codex"). */
  agent?: string;
  cwd?: "repo" | "worktree";
  profile?: string;
  /** Display name for rows added beyond the standard five. */
  name?: string;
}

export interface Stage {
  id: string;
  name: string;
}

/** The "no" side of a decision: other steps, or a loop back to an earlier one. */
export type DecisionNo = string[] | { repeatFrom: string; max: number };

export interface Step {
  id: string;
  name: string;
  stage: string;
  /** The surface (row) that does the step. */
  on: string;
  after?: string | string[];
  /** Agent steps. */
  prompt?: string;
  saves?: string;
  /** Shell steps. */
  run?: string;
  /** Browser steps. */
  open?: string;
  /** Steps on "you". */
  approve?: string;
  review?: string;
  ask?: string;
  /** A decision: a rule, or "ask" for an agent or you to answer `question`. */
  decide?: string;
  question?: string;
  yes?: string | string[];
  no?: DecisionNo;
  /** What happens after the last try of a repeat loop. */
  onLimit?: "ask" | "stop";
  doneWhen?: string;
  [key: string]: unknown;
}

/** A "for each" loop over a list a step produced. Repeat loops live on decisions. */
export interface ForEachLoop {
  id: string;
  forEach: string;
  as: string;
  steps: string[];
}

export interface ProcessEnd {
  after: string;
  name: string;
}

export interface Process {
  $schema: string;
  id: string;
  name: string;
  version: number;
  /** Where a process copied from a template or import came from. */
  from?: string;
  start: ProcessStart;
  surfaces: Surface[];
  stages: Stage[];
  steps: Step[];
  loops?: ForEachLoop[];
  ends?: ProcessEnd[];
  [key: string]: unknown;
}

// Versions ------------------------------------------------------------------

export interface VersionTag {
  text: string;
  tone: "current" | "run" | "compared" | "muted";
}

/** One line of `<id>.history.jsonl`, with the snapshot it resolves to. */
export interface ProcessVersion {
  version: number;
  message: string;
  by: string;
  /** ms since epoch */
  at: number;
  hash: string;
  process: Process;
  /** "restored v1", "template" and the like. Run tags are derived. */
  note?: string;
}

/** A process as the app holds it: its numbered versions and any draft. */
export interface ProcessRecord {
  id: string;
  versions: ProcessVersion[];
  /** Unsaved edits, autosaved. Null when the file matches the latest version. */
  draft: Process | null;
}

// Runs ----------------------------------------------------------------------

export type StepState = "pending" | "running" | "waiting" | "done" | "skipped" | "stopped";

export interface StepRun {
  state: StepState;
  note: string;
}

export type RunStatus = "running" | "waiting" | "done" | "stopped";

export interface Run {
  id: string;
  number: number;
  processId: string;
  /** The version the run started with; it always draws that one. */
  version: number;
  hash: string;
  process: Process;
  status: RunStatus;
  startedAt: number;
  endedAt?: number;
  /** When the run last changed status: how long it has been waiting or running. */
  since: number;
  /** "issue #142", "webhook", "folder" */
  trigger: string;
  /** What started it, for the lanes' corner: "TypeError in parseDate", a file name. */
  detail?: string;
  steps: Record<string, StepRun>;
  /** Per loop id: how far it got ("ran twice", "test 2 of 3"). */
  loops: Record<string, string>;
  /** Pane numbers in the Terminals tab, by surface id. */
  panes: Record<string, number>;
  /** How many times each repeat decision said no, and each for-each loop has gone round. */
  tries: Record<string, number>;
  /** Hand-written lane lines for the seeded runs, dropped once the run moves on. */
  laneNotes?: Record<string, { text: string; tone: Tone }>;
  /** What the waiting card says. */
  waitSummary?: string;
  /** How it ended, for the Recent list. */
  result?: string;
}

/** Colour roles. Components map these to theme tokens. */
export type Tone = "ok" | "warn" | "accent" | "muted" | "danger";

// Connections ---------------------------------------------------------------

export type ConnectionGroup = "work" | "local" | "systems";

export interface Connection {
  id: string;
  group: ConnectionGroup;
  icon: string;
  name: string;
  on: boolean;
  status: string;
  desc: string;
  used: string;
  /** Settings shown in the side panel, by field label. Secrets are never kept here. */
  settings: Record<string, string>;
  /** Last Test connection result. */
  test?: { ok: boolean; text: string };
}
