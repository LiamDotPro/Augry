---
id: T37
epic: 001-trigger-core
title: Add triggers to the reducer, the Config type and the helpers
status: done
depends_on: [T36]
size: M
model: haiku
files:
  - src/ipc/types.ts
  - src/state/reducer.ts
  - src/state/reducer.test.ts
  - src/lib/triggers.ts
  - src/lib/triggers.test.ts
---

## Goal

The frontend state knows about process triggers: `Config` has a `triggers` block, the reducer holds the last triggers status and a neutral notice, and `src/lib/triggers.ts` has the pure helpers for the toast text and the one line status of a process, with tests. No component or action uses them yet.

## Read first

- `plans/001-trigger-core/epic.md` Decision 16
- `src/state/reducer.ts:29-45` `DEFAULT_CONFIG`, `:100-180` `AppState` and `initialState`, `:180-230` the `Action` union
- `src/ipc/types.ts` the `Config` interface and the trigger types T36 added
- `src/lib/appearance.test.ts` the style of a small vitest file in `src/lib/`

## Steps

1. In `src/ipc/types.ts`, directly above `export interface Config {` add:

```ts
/** Process triggers; local to this machine (config.json), never synced. */
export interface TriggersConfig {
  enabled: boolean;
  /** Ids of the local process definitions the user armed. */
  armed: string[];
}

```

2. In the same file, in `interface Config`, add this field after `finishedColor: string | null;`, and add the `ConfigPatch` type right after the closing `}` of `interface Config`:

```ts
  triggers: TriggersConfig;
```

```ts

/** What `config_set` takes: any field of Config, and `triggers` may carry just some of its fields. */
export type ConfigPatch = Partial<Omit<Config, "triggers">> & { triggers?: Partial<TriggersConfig> };
```

3. In the same file, in `interface Commands`, change the line

```ts
  config_set: { in: { patch: Partial<Config> }; out: Config };
```

to

```ts
  config_set: { in: { patch: ConfigPatch }; out: Config };
```

4. In `src/state/reducer.ts` make these edits.

In the `import type { ... } from "../ipc/types";` list add `TriggersStatus,` after `TerminalInfo,`.

In `DEFAULT_CONFIG`, after `finishedColor: null,` add:

```ts
  triggers: { enabled: false, armed: [] },
```

Directly above `export interface AppState {` add:

```ts
/** A neutral message toast, such as "Trigger fs.file-saved fired for ...". */
export interface Notice {
  message: string;
  /** Epoch ms when it was raised; a repeat of the same text restarts the timer. */
  at: number;
}

```

In `interface AppState`, after the line `error: string | null;` add:

```ts
  notice: Notice | null;
  /** Process triggers: what Settings shows. Null until the first status arrives. */
  triggers: { status: TriggersStatus | null };
```

In `initialState`, after `error: null,` add:

```ts
  notice: null,
  triggers: { status: null },
```

In the `Action` union, after the line `| { type: "ui/error"; message: string | null }` add:

```ts
  | { type: "ui/notice"; notice: Notice | null }
  | { type: "triggers/status"; status: TriggersStatus }
```

In `reducer`, after the `case "ui/error":` block (its `return` line) add:

```ts

    case "ui/notice":
      return { ...state, notice: action.notice };

    case "triggers/status":
      return { ...state, triggers: { status: action.status } };
```

5. Create `src/lib/triggers.ts` with exactly:

```ts
import type { TriggerFiredEvent, TriggerProcessStatus, TriggerRuleState } from "../ipc/types";

/** How long the notice toast stays before it goes by itself. */
export const NOTICE_MS = 6000;

/** The toast text for a fired trigger, for example "Trigger fs.file-saved fired for Keep the docs tidy". */
export function firedNotice(event: TriggerFiredEvent): string {
  const text = `Trigger ${event.kind} fired for ${event.processName}`;
  return event.coalesced > 0 ? `${text} (and ${event.coalesced} more)` : text;
}

const STATE_WORDS: Record<Exclude<TriggerRuleState, "active">, string> = {
  rejected: "rejected",
  sourceFailed: "could not start",
  skipped: "skipped",
};

/** A rule id is `<processId>/<triggerId>` with an optional `#<n>`; the list shows the part after the slash. */
function triggerLabel(ruleId: string): string {
  const slash = ruleId.indexOf("/");
  return slash === -1 ? ruleId : ruleId.slice(slash + 1);
}

/**
 * The one line under a process in Settings: the problem with its file, or
 * whether it is armed, or how many of its rules run and why the others do not.
 */
export function processStatusLine(process: TriggerProcessStatus): string {
  if (process.problem) return process.problem;
  if (!process.armed) return "Not armed";
  if (process.rules.length === 0) return "Armed, no triggers";

  const parts: string[] = [];
  const active = process.rules.filter((rule) => rule.state === "active").length;
  if (active > 0) parts.push(active === 1 ? "1 rule active" : `${active} rules active`);
  for (const rule of process.rules) {
    if (rule.state === "active") continue;
    parts.push(`${triggerLabel(rule.id)} ${STATE_WORDS[rule.state]}: ${rule.reason ?? "no reason given"}`);
  }
  return parts.join("; ");
}
```

6. Create `src/lib/triggers.test.ts` with exactly:

```ts
import { describe, expect, it } from "vitest";
import type { TriggerFiredEvent, TriggerProcessStatus } from "../ipc/types";
import { firedNotice, processStatusLine } from "./triggers";

const fired = (coalesced: number): TriggerFiredEvent => ({
  processId: "tidy",
  processName: "Keep the docs tidy",
  ruleId: "tidy/saved",
  kind: "fs.file-saved",
  at: 1760000000123,
  payload: { path: "/x/a.md" },
  coalesced,
});

const process = (patch: Partial<TriggerProcessStatus>): TriggerProcessStatus => ({
  id: "tidy",
  name: "Keep the docs tidy",
  armed: true,
  rules: [],
  ...patch,
});

describe("firedNotice", () => {
  it("names the kind and the process", () => {
    expect(firedNotice(fired(0))).toBe("Trigger fs.file-saved fired for Keep the docs tidy");
  });

  it("says how many events were merged into this one", () => {
    expect(firedNotice(fired(3))).toBe("Trigger fs.file-saved fired for Keep the docs tidy (and 3 more)");
  });
});

describe("processStatusLine", () => {
  it("shows the problem first", () => {
    expect(processStatusLine(process({ problem: "name is missing", armed: true }))).toBe("name is missing");
  });

  it("says when a process is not armed", () => {
    expect(processStatusLine(process({ armed: false }))).toBe("Not armed");
  });

  it("says when an armed process has no triggers", () => {
    expect(processStatusLine(process({}))).toBe("Armed, no triggers");
  });

  it("counts active rules", () => {
    const rules = [
      { id: "tidy/a", kind: "fs.file-saved", state: "active" as const },
      { id: "tidy/b#2", kind: "fs.file-saved", state: "active" as const },
    ];
    expect(processStatusLine(process({ rules }))).toBe("2 rules active");
    expect(processStatusLine(process({ rules: rules.slice(0, 1) }))).toBe("1 rule active");
  });

  it("lists the rules that do not run, with the reason", () => {
    const rules = [
      { id: "tidy/a", kind: "fs.file-saved", state: "active" as const },
      { id: "tidy/dl", kind: "fs.download-finished", state: "skipped" as const, reason: "this machine has no downloads folder" },
      { id: "tidy/c", kind: "x.y", state: "rejected" as const, reason: "unknown kind x.y" },
      { id: "tidy/d", kind: "fs.file-saved", state: "sourceFailed" as const, reason: "cannot watch" },
    ];
    expect(processStatusLine(process({ rules }))).toBe(
      "1 rule active; dl skipped: this machine has no downloads folder; c rejected: unknown kind x.y; d could not start: cannot watch",
    );
  });

  it("starts with the first problem when nothing is active", () => {
    const rules = [{ id: "tidy/dl", kind: "fs.download-finished", state: "skipped" as const, reason: "no folder" }];
    expect(processStatusLine(process({ rules }))).toBe("dl skipped: no folder");
  });
});
```

7. In `src/state/reducer.test.ts`, change the import line

```ts
import { initialState, reducer, toSession, totalPages, visibleIds, type AppState } from "./reducer";
```

to

```ts
import { DEFAULT_CONFIG, initialState, reducer, toSession, totalPages, visibleIds, type AppState } from "./reducer";
```

and append this at the end of the file:

```ts

describe("triggers and notices", () => {
  it("starts with triggers off, no status and no notice", () => {
    expect(DEFAULT_CONFIG.triggers).toEqual({ enabled: false, armed: [] });
    expect(initialState.triggers.status).toBeNull();
    expect(initialState.notice).toBeNull();
  });

  it("keeps the last status", () => {
    const status = { enabled: true, processes: [] };
    const state = reducer(initialState, { type: "triggers/status", status });
    expect(state.triggers.status).toBe(status);
  });

  it("raises and clears a notice without touching the error toast", () => {
    let state = reducer(initialState, { type: "ui/error", message: "boom" });
    state = reducer(state, { type: "ui/notice", notice: { message: "Trigger fired", at: 5 } });
    expect(state.notice).toEqual({ message: "Trigger fired", at: 5 });
    expect(state.error).toBe("boom");
    state = reducer(state, { type: "ui/notice", notice: null });
    expect(state.notice).toBeNull();
    expect(state.error).toBe("boom");
  });
});
```

## Acceptance

- [ ] `npm test` passes; `src/lib/triggers.test.ts` shows 8 tests and `src/state/reducer.test.ts` shows 3 more than before
- [ ] `npm run build` exits 0
- [ ] `git status --porcelain` lists only paths under `files:`

## Out of scope

- Calling any trigger command or subscribing to the event. T38 does.
- Rendering the notice or the Settings rows. T38 and T39 do.
- Saving the notice or the status in the session. They are not part of `toSession`.
- A component test or a new test library.

## If stuck

- If `npm run build` reports another file that builds a `Config` object without `triggers`, add `triggers: { enabled: false, armed: [] },` to that object only if it is a test fixture or default; if it is anything else set status blocked and say where.
- If the import list in `reducer.ts` is laid out differently, add `TriggersStatus` in the same list in alphabetical position.
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed (npm test 174 passed, triggers.test 8, reducer.test 40; npm run build exit 0; first build caught a missing TriggersConfig interface, added)
- 2026-10-12 checker: pass
