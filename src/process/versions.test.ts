import { describe, expect, it } from "vitest";
import { BUG_FIX, FEATURE_DELIVERY, featureDeliveryHistory, featureDeliveryV6 } from "./examples";
import { updateStep } from "./edit";
import { hashOf } from "./json";
import { initialState, reducer, type ProcessState } from "./state";
import type { ProcessRecord } from "./types";
import { changeSentence, latest, summarizeChanges, versionDiff, versionTags, withDraft } from "./versions";

const NOW = new Date(2026, 9, 11, 11, 0).getTime();

/** Feature delivery with its seven versions, the way it would come off disk. */
function featureDelivery(): ProcessRecord {
  return {
    id: "feature-delivery",
    draft: null,
    versions: featureDeliveryHistory().map((process, i) => ({
      version: i + 1,
      message: `v${i + 1}`,
      by: "You",
      at: NOW,
      hash: hashOf(process),
      process,
      ...(i === 2 ? { note: "restored v1" } : {}),
    })),
  };
}

/** The tab after reading `records` from disk. */
const loaded = (...records: ProcessRecord[]): ProcessState => reducer(initialState(), { type: "loaded", records, problems: [] });

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
    const record = featureDelivery();
    const edited = withDraft(record, updateStep(latest(record).process, "plan", { name: "Plan it" }));
    expect(edited.draft).not.toBeNull();
    expect(withDraft(edited, latest(record).process).draft).toBeNull();
  });

  it("saves a draft as the next version and restores old ones as new versions", () => {
    let state = loaded(featureDelivery());
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

  it("tags the current version, the compared one and restores", () => {
    const record = featureDelivery();
    const tags = (n: number) => versionTags(record, record.versions.find((v) => v.version === n)!, [], 6).map((t) => t.text);
    expect(tags(7)).toEqual(["current"]);
    expect(tags(6)).toEqual(["compared"]);
    expect(tags(3)).toEqual(["restored v1"]);
  });
});

describe("reducer", () => {
  it("starts empty and waits for the disk", () => {
    const state = initialState();
    expect(state.records).toEqual([]);
    expect(state.runs).toEqual([]);
    expect(state.disk.status).toBe("loading");
    expect(state.connections.every((c) => !c.on && c.status === "Not set up")).toBe(true);
    expect(loaded().disk.status).toBe("ready");
  });

  it("Use template saves the template as version 1 of yours and opens it", () => {
    let state = loaded(featureDelivery());
    state = reducer(state, { type: "use-template", process: { ...FEATURE_DELIVERY, from: "template:feature-delivery@3" }, name: "Feature delivery", now: NOW });
    const record = state.records[state.records.length - 1]!;
    expect(record.id).toBe("feature-delivery-2");
    expect(record.versions).toHaveLength(1);
    expect(latest(record)).toMatchObject({ message: "Created from the Feature delivery template", by: "You" });
    expect(latest(record).process).toMatchObject({ id: "feature-delivery-2", version: 1, from: "template:feature-delivery@3" });
    expect(state.route).toEqual({ view: "editor", processId: record.id });
  });

  it("adds, copies and deletes processes", () => {
    let state = loaded();
    state = reducer(state, { type: "new-process", now: NOW });
    state = reducer(state, { type: "import", process: BUG_FIX, now: NOW });
    expect(state.records.map((r) => r.id)).toEqual(["new-process", "bug-fix"]);
    state = reducer(state, { type: "duplicate", processId: "bug-fix", now: NOW });
    expect(state.records.map((r) => r.id)).toEqual(["new-process", "bug-fix", "bug-fix-2"]);
    state = reducer(state, { type: "delete", processId: "new-process" });
    expect(state.records.map((r) => r.id)).toEqual(["bug-fix", "bug-fix-2"]);
    expect(state.route).toEqual({ view: "home" });
  });

  it("says when the processes folder couldn't be read", () => {
    const state = reducer(initialState(), { type: "load-failed", error: "permission denied" });
    expect(state.disk).toEqual({ status: "failed", problems: [], error: "permission denied" });
  });
});
