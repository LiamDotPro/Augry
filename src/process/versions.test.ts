import { describe, expect, it } from "vitest";
import { FEATURE_DELIVERY, featureDeliveryV6 } from "./examples";
import { updateStep } from "./edit";
import { initialState, reducer } from "./state";
import { changeSentence, latest, summarizeChanges, versionDiff, versionTags, withDraft } from "./versions";

const NOW = new Date(2026, 9, 11, 11, 0).getTime();

describe("summarizeChanges", () => {
  it("says what changed from v6 to v7, as the Compare chips do", () => {
    const changes = summarizeChanges(featureDeliveryV6(), FEATURE_DELIVERY).map((c) => `${c.sign} ${c.text}`);
    expect(changes).toEqual([
      "+ Summarise changes on Claude Code",
      "~ Run checks: command",
      "~ Checks pass?: max 2 → 3",
      "~ Open PR: command",
    ]);
  });

  it("reads renames as a sentence", () => {
    const edited = updateStep(updateStep(FEATURE_DELIVERY, "smoke", { name: "Settings check" }), "smoke", { open: "localhost:5173/" });
    const changes = summarizeChanges(FEATURE_DELIVERY, edited);
    expect(changeSentence(changes)).toBe("Renamed Smoke test to Settings check, Settings check: page");
  });
});

describe("versionDiff", () => {
  it("folds unchanged lines and keeps two lines of context", () => {
    const rows = versionDiff(featureDeliveryV6(), FEATURE_DELIVERY);
    expect(rows[0]).toMatchObject({ kind: "fold" });
    expect(rows.filter((r) => r.kind === "add").length).toBeGreaterThan(0);
    expect(rows.filter((r) => r.kind === "del").some((r) => r.kind === "del" && r.text.includes('"version": 6'))).toBe(true);
  });
});

describe("drafts and versions", () => {
  it("drops the draft when an edit goes back to the saved version", () => {
    const state = initialState(NOW);
    const record = state.records[0]!;
    const edited = withDraft(record, updateStep(latest(record).process, "plan", { name: "Plan it" }));
    expect(edited.draft).not.toBeNull();
    expect(withDraft(edited, latest(record).process).draft).toBeNull();
  });

  it("saves a draft as the next version and restores old ones as new versions", () => {
    let state = initialState(NOW);
    const id = "feature-delivery";
    const current = latest(state.records[0]!).process;
    state = reducer(state, { type: "edit", processId: id, process: updateStep(current, "smoke", { name: "Settings check" }) });
    state = reducer(state, { type: "save", processId: id, message: "Renamed Smoke test", now: NOW });
    let record = state.records.find((r) => r.id === id)!;
    expect(latest(record)).toMatchObject({ version: 8, message: "Renamed Smoke test" });
    expect(latest(record).process.version).toBe(8);
    expect(record.draft).toBeNull();

    state = reducer(state, { type: "restore", processId: id, version: 6, now: NOW });
    record = state.records.find((r) => r.id === id)!;
    expect(latest(record)).toMatchObject({ version: 9, message: "Restored v6", note: "restored v6" });
    expect(latest(record).process.steps.some((s) => s.id === "summary")).toBe(false);
    expect(record.versions).toHaveLength(9);
  });

  it("tags versions with the runs that used them", () => {
    const state = initialState(NOW);
    const record = state.records[0]!;
    const tags = (n: number) => versionTags(record, record.versions.find((v) => v.version === n)!, state.runs, 6).map((t) => t.text);
    expect(tags(7)).toEqual(["current"]);
    expect(tags(6)).toEqual(["run 4", "compared"]);
    expect(tags(5)).toEqual(["runs 1–3"]);
    expect(tags(3)).toEqual(["restored v1"]);
  });
});

describe("reducer", () => {
  it("Run with a draft saves it first, then opens the run", () => {
    let state = initialState(NOW);
    const id = "bug-fix";
    const current = latest(state.records[1]!).process;
    state = reducer(state, { type: "edit", processId: id, process: updateStep(current, "fix", { name: "Fix it" }) });
    state = reducer(state, { type: "run", processId: id, now: NOW });
    const run = state.runs[0]!;
    expect(run).toMatchObject({ processId: id, number: 13, version: 4 });
    expect(state.route).toEqual({ view: "run", runId: run.id });
    expect(state.records[1]!.draft).toBeNull();
  });

  it("Copy and run adds the template as version 1 and starts it", () => {
    let state = initialState(NOW);
    state = reducer(state, { type: "copy-template", templateId: "feature-delivery", run: true, trigger: "issue #139", now: NOW });
    const record = state.records[state.records.length - 1]!;
    expect(record.id).toBe("feature-delivery-2");
    expect(record.versions).toHaveLength(1);
    expect(latest(record).process).toMatchObject({ version: 1, from: "template:feature-delivery@3" });
    expect(state.runs[0]).toMatchObject({ processId: record.id, number: 1, trigger: "issue #139" });
    expect(state.route).toEqual({ view: "run", runId: state.runs[0]!.id });
  });

  it("Approve and Send back change the waiting run", () => {
    let state = initialState(NOW);
    state = reducer(state, { type: "approve", runId: "feature-delivery#4", stepId: "review", now: NOW });
    const run = state.runs.find((r) => r.id === "feature-delivery#4")!;
    expect(run.steps.review!.state).toBe("done");
    expect(run.steps.pr!.state).toBe("running");
    expect(run.status).toBe("running");
  });

  it("a file arriving starts the watched-folder process and says so", () => {
    let state = initialState(NOW);
    state = reducer(state, { type: "file-arrived", processId: "crash-report", now: NOW });
    expect(state.runs[0]).toMatchObject({ processId: "crash-report", number: 4, trigger: "folder", detail: "crash-2026-10-11.log" });
    expect(state.notice).toMatchObject({ title: "Crash report started", folder: "~/crash-dumps" });
  });

  it("tests and saves a connection", () => {
    let state = initialState(NOW);
    state = reducer(state, { type: "connection-test", id: "mysql" });
    expect(state.connections.find((c) => c.id === "mysql")!.test).toEqual({ ok: false, text: "Add a host and port first" });
    state = reducer(state, { type: "connection-set", id: "mysql", key: "host", value: "localhost:3306" });
    state = reducer(state, { type: "connection-save", id: "mysql" });
    expect(state.connections.find((c) => c.id === "mysql")).toMatchObject({ on: true, status: "Connected · localhost:3306" });
  });
});
