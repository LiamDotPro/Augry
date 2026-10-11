/**
 * The processes and templates the design shows (Components.dc.html,
 * Process Tab.dc.html), as `augry/process@1` objects. Until the backend reads
 * `.augry/processes/`, these are the tab's data.
 */

import { cloneProcess } from "./json";
import { PROCESS_SCHEMA, type Process, type StartKind, type Surface } from "./types";

/** The five rows every new process starts with. */
export const STANDARD_SURFACES: Surface[] = [
  { id: "you", kind: "you" },
  { id: "claude", kind: "terminal", agent: "claude-code", cwd: "repo" },
  { id: "codex", kind: "terminal", agent: "codex", cwd: "worktree" },
  { id: "shell", kind: "terminal", cwd: "worktree" },
  { id: "browser", kind: "browser", profile: "controlled" },
];

const surfaces = () => cloneProcess(STANDARD_SURFACES);

export const FEATURE_DELIVERY: Process = {
  $schema: PROCESS_SCHEMA,
  id: "feature-delivery",
  name: "Feature delivery",
  version: 7,
  start: { on: "ticket", source: "github", query: "is:open label:ready", as: "ticket" },
  surfaces: surfaces(),
  stages: [
    { id: "plan", name: "Plan" },
    { id: "build", name: "Build" },
    { id: "ship", name: "Ship" },
  ],
  steps: [
    { id: "plan", name: "Write plan", stage: "plan", on: "claude", prompt: "Write a plan for {{ticket.title}}", saves: "plan.md" },
    { id: "approve", name: "Approve plan", stage: "plan", on: "you", after: "plan", approve: "plan.md" },
    { id: "impl", name: "Implement", stage: "build", on: "codex", after: "approve", prompt: "Implement plan.md" },
    { id: "checks", name: "Run checks", stage: "build", on: "shell", after: "impl", run: "pnpm check && pnpm test" },
    {
      id: "passed", name: "Checks pass?", stage: "build", on: "shell", after: "checks", decide: "checks.exitCode == 0",
      yes: ["smoke", "summary"], no: { repeatFrom: "impl", max: 3 },
    },
    { id: "smoke", name: "Smoke test", stage: "build", on: "browser", open: "localhost:5173/settings", saves: "screenshots/" },
    { id: "summary", name: "Summarise changes", stage: "build", on: "claude", prompt: "Summarise the diff for the PR", saves: "summary.md" },
    { id: "review", name: "Review changes", stage: "ship", on: "you", after: ["smoke", "summary"], review: "diff + screenshots" },
    { id: "pr", name: "Open PR", stage: "ship", on: "shell", after: "review", run: "gh pr create --body-file summary.md" },
  ],
  ends: [{ after: "pr", name: "PR open" }],
};

export const BUG_FIX: Process = {
  $schema: PROCESS_SCHEMA,
  id: "bug-fix",
  name: "Bug fix",
  version: 3,
  start: { on: "webhook", path: "/hooks/bug-fix", from: "Sentry", reach: "relay", as: "alert" },
  surfaces: surfaces(),
  stages: [
    { id: "triage", name: "Triage" },
    { id: "fix", name: "Fix" },
    { id: "ship", name: "Ship" },
  ],
  steps: [
    { id: "repro", name: "Reproduce", stage: "triage", on: "claude", prompt: "Write failing tests for {{alert.title}}", saves: "repro.md" },
    {
      id: "found", name: "Reproduced?", stage: "triage", on: "claude", after: "repro", decide: "ask",
      question: "Did a test fail for this bug?", yes: ["fix"], no: ["info"],
    },
    { id: "info", name: "Add details", stage: "triage", on: "you", ask: "What else do we know about this?" },
    { id: "fix", name: "Fix test", stage: "fix", on: "codex", prompt: "Make {{test}} pass" },
    { id: "test", name: "Run test", stage: "fix", on: "shell", after: "fix", run: "pnpm vitest {{test}}" },
    {
      id: "green", name: "Passes?", stage: "fix", on: "shell", after: "test", decide: "test.exitCode == 0",
      yes: ["verify"], no: { repeatFrom: "fix", max: 3 },
    },
    { id: "verify", name: "Check the page", stage: "ship", on: "browser", open: "{{alert.url}}" },
    { id: "review", name: "Review changes", stage: "ship", on: "you", after: "verify", review: "diff" },
    { id: "pr", name: "Open PR", stage: "ship", on: "shell", after: "review", run: "gh pr create --fill" },
  ],
  loops: [{ id: "each", forEach: "repro.failingTests", as: "test", steps: ["fix", "test", "green"] }],
  ends: [
    { after: "pr", name: "PR open" },
    { after: "info", name: "Needs info" },
  ],
};

/** Bug fix, started by a watched folder instead of a webhook (the design derives it the same way). */
export const CRASH_REPORT: Process = (() => {
  const text = JSON.stringify(BUG_FIX)
    .split("alert.title")
    .join("file.name")
    .split("{{alert.url}}")
    .join("localhost:5173")
    .split("alert")
    .join("file");
  const p = JSON.parse(text) as Process;
  return {
    ...p,
    id: "crash-report",
    name: "Crash report",
    version: 1,
    start: { on: "folder", path: "~/crash-dumps", match: "*.log", when: "added", as: "file" },
  };
})();

/** Feature delivery as it was at v6, before Summarise changes. */
export function featureDeliveryV6(): Process {
  const p = cloneProcess(FEATURE_DELIVERY);
  p.version = 6;
  p.steps = p.steps.filter((s) => s.id !== "summary");
  const by = Object.fromEntries(p.steps.map((s) => [s.id, s]));
  by.checks!.run = "pnpm test";
  by.passed!.yes = ["smoke"];
  by.passed!.no = { repeatFrom: "impl", max: 2 };
  by.review!.after = "smoke";
  by.pr!.run = "gh pr create --fill";
  return p;
}

/** The earlier versions of Feature delivery, oldest first, so its history reads true. */
export function featureDeliveryHistory(): Process[] {
  const v6 = featureDeliveryV6();
  const v5 = cloneProcess(v6);
  v5.version = 5;
  v5.steps = v5.steps.filter((s) => s.id !== "smoke");
  const passed5 = v5.steps.find((s) => s.id === "passed")!;
  passed5.yes = ["review"];
  delete v5.steps.find((s) => s.id === "review")!.after;
  const v4 = cloneProcess(v5);
  v4.version = 4;
  v4.steps = v4.steps.filter((s) => s.id !== "approve");
  v4.steps.find((s) => s.id === "impl")!.after = "plan";
  const v3 = cloneProcess(v4);
  v3.version = 3;
  v3.surfaces.find((s) => s.id === "shell")!.cwd = "repo";
  const v2 = cloneProcess(v3);
  v2.version = 2;
  v2.steps.find((s) => s.id === "checks")!.run = "pnpm test --changed";
  const v1 = cloneProcess(v3);
  v1.version = 1;
  v1.from = "template:feature-delivery@3";
  v1.stages = [
    { id: "plan", name: "Planning" },
    { id: "build", name: "Building" },
    { id: "ship", name: "Shipping" },
  ];
  return [v1, v2, v3, v4, v5, v6, cloneProcess(FEATURE_DELIVERY)];
}

// Templates ------------------------------------------------------------------------

export interface Template {
  id: string;
  name: string;
  desc: string;
  /** What the chip says: "Ticket", "Weekly", "On push". */
  chip: string;
  start: StartKind;
  /** Template revision, recorded as `from` on copies. */
  revision: number;
  process: Process;
}

const base = (id: string, name: string, start: Process["start"]): Pick<Process, "$schema" | "id" | "name" | "version" | "start" | "surfaces"> => ({
  $schema: PROCESS_SCHEMA,
  id,
  name,
  version: 1,
  start,
  surfaces: surfaces(),
});

const PARALLEL: Process = {
  ...base("parallel-attempts", "Parallel attempts", { on: "manual", as: "task" }),
  stages: [
    { id: "try", name: "Try" },
    { id: "pick", name: "Pick" },
  ],
  steps: [
    { id: "trees", name: "Make worktrees", stage: "try", on: "shell", run: "git worktree add ../a && git worktree add ../b" },
    { id: "claude", name: "Attempt A", stage: "try", on: "claude", after: "trees", prompt: "{{task.text}}" },
    { id: "codex", name: "Attempt B", stage: "try", on: "codex", after: "trees", prompt: "{{task.text}}" },
    { id: "checks", name: "Check both", stage: "try", on: "shell", after: ["claude", "codex"], run: "pnpm check" },
    { id: "pick", name: "Pick the better one", stage: "pick", on: "you", after: "checks", review: "both diffs" },
    { id: "pr", name: "Open PR", stage: "pick", on: "shell", after: "pick", run: "gh pr create --fill" },
  ],
  ends: [{ after: "pr", name: "PR open" }],
};

const DEPENDENCIES: Process = {
  ...base("dependency-update", "Dependency update", { on: "schedule", cron: "weekly mon 09:00", as: "schedule" }),
  stages: [
    { id: "update", name: "Update" },
    { id: "ship", name: "Ship" },
  ],
  steps: [
    { id: "bump", name: "Update packages", stage: "update", on: "shell", run: "pnpm update --latest" },
    { id: "checks", name: "Run checks", stage: "update", on: "shell", after: "bump", run: "pnpm check && pnpm test" },
    { id: "passed", name: "All pass?", stage: "update", on: "shell", after: "checks", decide: "checks.exitCode == 0", yes: ["notes"], no: [] },
    { id: "notes", name: "Write PR notes", stage: "ship", on: "claude", prompt: "List what changed in the lockfile", saves: "notes.md" },
    { id: "pr", name: "Open PR", stage: "ship", on: "shell", after: "notes", run: "gh pr create --body-file notes.md" },
  ],
  ends: [
    { after: "pr", name: "PR open" },
    { after: "passed", name: "Nothing opened" },
  ],
};

const VISUAL: Process = {
  ...base("visual-check", "Visual check", { on: "branch", event: "push", as: "push" }),
  stages: [
    { id: "shots", name: "Screenshots" },
    { id: "report", name: "Report" },
  ],
  steps: [
    { id: "serve", name: "Start the app", stage: "shots", on: "shell", run: "pnpm dev" },
    { id: "shoot", name: "Screenshot page", stage: "shots", on: "browser", after: "serve", open: "localhost:5173{{page}}", saves: "shots/" },
    { id: "compare", name: "Compare with main", stage: "shots", on: "browser", after: "shoot", open: "shots/ vs main" },
    { id: "describe", name: "Describe changes", stage: "report", on: "claude", after: "compare", prompt: "Describe what looks different", saves: "visual.md" },
    { id: "look", name: "Look it over", stage: "report", on: "you", after: "describe", review: "visual.md + shots" },
  ],
  loops: [{ id: "pages", forEach: "pages", as: "page", steps: ["shoot", "compare"] }],
  ends: [{ after: "look", name: "Checked" }],
};

const RELEASE: Process = {
  ...base("release", "Release", { on: "manual", as: "release" }),
  stages: [
    { id: "prepare", name: "Prepare" },
    { id: "publish", name: "Publish" },
  ],
  steps: [
    { id: "bump", name: "Bump version", stage: "prepare", on: "shell", run: "pnpm version {{release.kind}}" },
    { id: "log", name: "Write changelog", stage: "prepare", on: "claude", after: "bump", prompt: "Write the changelog since the last tag", saves: "CHANGELOG.md" },
    { id: "ok", name: "Approve changelog", stage: "prepare", on: "you", after: "log", approve: "CHANGELOG.md" },
    { id: "tag", name: "Tag", stage: "publish", on: "shell", after: "ok", run: "git tag v{{version}}" },
    { id: "build", name: "Build", stage: "publish", on: "shell", after: "tag", run: "pnpm build" },
    { id: "ship", name: "Publish release", stage: "publish", on: "shell", after: "build", run: "gh release create v{{version}}" },
  ],
  ends: [{ after: "ship", name: "Released" }],
};

export const TEMPLATES: Template[] = [
  {
    id: "feature-delivery", name: "Feature delivery", desc: "Plan, build, check, review, then open a PR.", chip: "Ticket",
    start: "ticket", revision: 3, process: { ...cloneProcess(FEATURE_DELIVERY), version: 1 },
  },
  {
    id: "bug-fix", name: "Bug fix", desc: "Reproduce with failing tests, fix each one, check the page.", chip: "Webhook",
    start: "webhook", revision: 2, process: { ...cloneProcess(BUG_FIX), version: 1 },
  },
  {
    id: "parallel-attempts", name: "Parallel attempts", desc: "One task to Claude Code and Codex in two worktrees. You keep the better one.",
    chip: "Manual", start: "manual", revision: 1, process: PARALLEL,
  },
  {
    id: "dependency-update", name: "Dependency update", desc: "Update packages, run checks, open a PR only if everything passes.",
    chip: "Weekly", start: "schedule", revision: 1, process: DEPENDENCIES,
  },
  {
    id: "visual-check", name: "Visual check", desc: "Screenshot the key pages and compare them with main.", chip: "On push",
    start: "branch", revision: 1, process: VISUAL,
  },
  {
    id: "release", name: "Release", desc: "Bump the version, write the changelog, tag and build.", chip: "Manual",
    start: "manual", revision: 1, process: RELEASE,
  },
];

/** A new, empty process: one stage, one step, started by hand. */
export function blankProcess(id: string, name: string): Process {
  return {
    $schema: PROCESS_SCHEMA,
    id,
    name,
    version: 1,
    start: { on: "manual", as: "input" },
    surfaces: surfaces(),
    stages: [{ id: "stage-1", name: "Stage 1" }],
    steps: [{ id: "first", name: "First step", stage: "stage-1", on: "claude", prompt: "{{input.text}}" }],
    ends: [{ after: "first", name: "Done" }],
  };
}
