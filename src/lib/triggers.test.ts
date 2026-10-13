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
