/**
 * Small readings of a process object shared by every view: what a row is
 * called and how it is drawn, what starts the process, the one-line summary of
 * a step. Port of the helpers in Components.dc.html (use="lanes").
 */

import type { ProcessStart, Process, StartKind, Step, Surface, Tone } from "./types";

/** Colour family of a row. process.css defines `--lane-<tone>-rgb` for each. */
export type LaneTone = "you" | "claude" | "codex" | "shell" | "browser";

export interface SurfaceMeta {
  name: string;
  icon: string;
  tone: LaneTone;
  /** "You" gets a round tile, like an avatar. */
  round: boolean;
}

const STANDARD: Record<string, SurfaceMeta> = {
  you: { name: "You", icon: "gate", tone: "you", round: true },
  claude: { name: "Claude Code", icon: "claude", tone: "claude", round: false },
  codex: { name: "Codex", icon: "openai", tone: "codex", round: false },
  shell: { name: "Shell", icon: "term", tone: "shell", round: false },
  browser: { name: "Browser", icon: "globe", tone: "browser", round: false },
};

/** How a row is drawn: the standard five by id, anything added by its kind. */
export function surfaceMeta(surface: Surface | undefined): SurfaceMeta {
  if (!surface) return STANDARD.shell!;
  const standard = STANDARD[surface.id];
  const base =
    standard ??
    (surface.kind === "you"
      ? STANDARD.you!
      : surface.kind === "browser"
        ? STANDARD.browser!
        : surface.agent?.includes("claude")
          ? STANDARD.claude!
          : surface.agent?.includes("codex")
            ? STANDARD.codex!
            : STANDARD.shell!);
  return surface.name ? { ...base, name: surface.name } : base;
}

/** The meta of the row a step is on. */
export function stepSurface(process: Process, step: Step): SurfaceMeta {
  return surfaceMeta(process.surfaces.find((s) => s.id === step.on));
}

/** What the row's sub line says while editing. */
export function surfaceDetail(surface: Surface): string {
  if (surface.kind === "you") return "approvals and answers";
  if (surface.kind === "browser") return "controlled Chrome";
  return `new pane · ${surface.cwd === "worktree" ? "worktree" : "repo root"}`;
}

/** `after`, `yes` and friends take one id or several. */
export function list(value: string | string[] | undefined | null): string[] {
  if (value == null) return [];
  return Array.isArray(value) ? value : [value];
}

export function isRepeat(no: Step["no"]): no is { repeatFrom: string; max: number } {
  return !!no && !Array.isArray(no) && typeof no === "object" && typeof no.repeatFrom === "string";
}

/** The one-line summary under a step's name. */
export function lineOf(step: Step): string {
  if (step.prompt) return "› " + step.prompt;
  if (step.run) return "$ " + step.run;
  if (step.open) return "↗ " + step.open;
  if (step.approve) return "approve " + step.approve;
  if (step.review) return "review " + step.review;
  if (step.ask) return "asks: " + step.ask;
  if (step.decide === "ask") return "asks: " + (step.question ?? "");
  return step.decide ?? "";
}

// Starting points ------------------------------------------------------------

export interface StartKindInfo {
  kind: StartKind;
  icon: string;
  name: string;
  detail: string;
}

/** The "Starts from" picker, in the design's order. */
export const START_KINDS: StartKindInfo[] = [
  { kind: "ticket", icon: "ticket", name: "Ticket", detail: "A GitHub, Linear or Jira issue that matches a filter" },
  { kind: "webhook", icon: "webhook", name: "Webhook", detail: "A request to a local address, or through the relay" },
  { kind: "manual", icon: "manual", name: "Manual", detail: "You press Start run and say what it is for" },
  { kind: "schedule", icon: "schedule", name: "Schedule", detail: "At set times, like weekdays at 09:00" },
  { kind: "branch", icon: "branch", name: "Git", detail: "On push, or when a PR opens or gets a review" },
  { kind: "notes", icon: "notes", name: "Note or selection", detail: "A note saved to a folder, or text sent from a pane" },
  { kind: "folder", icon: "folder", name: "Folder", detail: "A file is added to or changed in a folder you choose" },
];

export function startKindInfo(kind: StartKind): StartKindInfo {
  return START_KINDS.find((k) => k.kind === kind) ?? START_KINDS[2]!;
}

/** The long title in the lanes' top-left corner and the runbook. */
export function startTitle(start: ProcessStart): string {
  switch (start.on) {
    case "ticket":
      return "GitHub issue · " + (start.query ?? "");
    case "webhook":
      return "Webhook · POST " + (start.path ?? "");
    case "manual":
      return "Manual start";
    case "schedule":
      return "Schedule · " + (start.cron ?? "");
    case "branch":
      return "Git · " + (start.event ?? "on push");
    case "notes":
      return "Note · " + (start.path ?? "saved to a folder");
    case "folder":
      return "New " + (start.match ?? "file") + " in " + (start.path ?? "");
  }
}

/** The short chip on a process card. */
export function startChip(start: ProcessStart): string {
  switch (start.on) {
    case "ticket":
      return "GitHub issue";
    case "webhook":
      return "Webhook";
    case "folder":
      return "Watched folder";
    case "schedule":
      return start.cron?.startsWith("weekly") ? "Weekly" : "Schedule";
    case "branch":
      return "On push";
    case "notes":
      return "Note";
    case "manual":
      return "Manual";
  }
}

/** What a new starting point of a kind begins with. */
export function defaultStart(kind: StartKind, previous: ProcessStart): ProcessStart {
  const keepAs = (fallback: string) => ({ as: previous.on === kind ? previous.as : fallback });
  switch (kind) {
    case "ticket":
      return { on: kind, source: "github", query: "is:open label:ready", ...keepAs("ticket") };
    case "webhook":
      return { on: kind, path: "/hooks/new", reach: "local", ...keepAs("request") };
    case "manual":
      return { on: kind, ...keepAs("input") };
    case "schedule":
      return { on: kind, cron: "weekdays 09:00", ...keepAs("schedule") };
    case "branch":
      return { on: kind, event: "push", ...keepAs("git") };
    case "notes":
      return { on: kind, path: "~/notes/inbox", ...keepAs("note") };
    case "folder":
      return { on: kind, path: "~/Downloads", match: "*", when: "added", ...keepAs("file") };
  }
}

/** A run's state as a colour role. */
export function toneOfState(state: string): Tone {
  if (state === "done") return "ok";
  if (state === "waiting") return "warn";
  if (state === "running") return "accent";
  if (state === "stopped") return "danger";
  return "muted";
}

/** A file name for a process id. */
export function processFile(id: string): string {
  return `.augry/processes/${id}.json`;
}

/** A url-safe id from a name, unique among `taken`. */
export function uniqueId(name: string, taken: Iterable<string>): string {
  const base =
    name
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, "-")
      .replace(/^-|-$/g, "") || "process";
  const used = new Set(taken);
  if (!used.has(base)) return base;
  let n = 2;
  while (used.has(`${base}-${n}`)) n++;
  return `${base}-${n}`;
}
