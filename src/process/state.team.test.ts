import { describe, expect, it } from "vitest";
import type { TeamProcess } from "../cloud/types";
import { BUG_FIX } from "./examples";
import { cloneProcess } from "./json";
import { initialState, recordOf, reducer, spaceRecords, teamOf, type Action, type ProcessState } from "./state";
import { latest } from "./versions";

const NOW = new Date(2026, 9, 12, 12, 0).getTime();

function tp(id: string, seq: number, version: number, name = "Fix a GitHub issue", by = "u-maya"): TeamProcess {
  return {
    id,
    teamId: "t1",
    name,
    version,
    seq,
    deleted: false,
    updatedBy: by,
    updatedAt: new Date(NOW - seq * 60000).toISOString(),
    definition: { ...cloneProcess(BUG_FIX), id: "fix-issue", name, version },
  };
}

const run = (state: ProcessState, ...actions: Action[]) => actions.reduce(reducer, state);

function withTeam(): ProcessState {
  return run(initialState(), { type: "loaded", records: [], problems: [] }, { type: "team-page", teamId: "t1", page: { processes: [tp("p1", 1, 3), tp("p2", 2, 5, "Release")], lastSeq: 2, hasMore: false } });
}

describe("team spaces", () => {
  it("shows the selected space's processes", () => {
    let state = run(withTeam(), { type: "new-process", now: NOW }, { type: "navigate", route: { view: "home" } });
    expect(spaceRecords(state).map((r) => r.id)).toEqual(["new-process"]);
    state = run(state, { type: "space", space: "t1" });
    expect(spaceRecords(state).map((r) => r.id)).toEqual(["p1", "p2"]);
    expect(teamOf(state, "p1")).toBe("t1");
    expect(teamOf(state, "new-process")).toBeNull();
  });

  it("goes home when the open process belongs to the other space", () => {
    let state = run(withTeam(), { type: "space", space: "t1" }, { type: "navigate", route: { view: "editor", processId: "p1" } });
    state = run(state, { type: "space", space: "personal" });
    expect(state.route).toEqual({ view: "home" });
    state = run(state, { type: "navigate", route: { view: "connections" } }, { type: "space", space: "t1" });
    expect(state.route).toEqual({ view: "connections" });
  });

  it("never saves a team draft as a local version", () => {
    let state = run(withTeam(), { type: "space", space: "t1" });
    const record = recordOf(state, "p1")!;
    state = run(state, { type: "edit", processId: "p1", process: { ...latest(record).process, name: "Mine" } });
    // Team saves go to the server (ProcessView); the reducer's local save leaves a team record alone.
    const after = recordOf(state, "p1")!;
    expect(after.versions.map((v) => v.version)).toEqual([3]);
    expect(after.draft?.name).toBe("Mine");
  });
});

describe("team writes", () => {
  it("saving clears the draft and adds the version", () => {
    let state = run(withTeam(), { type: "space", space: "t1" });
    state = run(state, { type: "edit", processId: "p1", process: { ...latest(recordOf(state, "p1")!).process, name: "Mine" } });
    state = run(state, { type: "team-saved", teamId: "t1", process: tp("p1", 3, 4, "Mine", "u-me") });
    const record = recordOf(state, "p1")!;
    expect(record.draft).toBeNull();
    expect(record.versions.map((v) => v.version)).toEqual([3, 4]);
    expect(state.teams.t1!.cache.processes.p1!.version).toBe(4);
  });

  it("a conflict keeps the draft on top of theirs and asks", () => {
    let state = run(withTeam(), { type: "space", space: "t1" });
    state = run(state, { type: "edit", processId: "p1", process: { ...latest(recordOf(state, "p1")!).process, name: "Mine" } });
    state = run(state, { type: "team-conflict", teamId: "t1", processId: "p1", current: tp("p1", 4, 4) });
    const record = recordOf(state, "p1")!;
    expect(latest(record).version).toBe(4);
    expect(record.draft?.name).toBe("Mine");
    expect(state.teamDialog).toMatchObject({ kind: "conflict", processId: "p1", base: { version: 3, byId: "u-maya" } });
    // Saving it again closes the dialog.
    state = run(state, { type: "team-saved", teamId: "t1", process: tp("p1", 5, 5, "Mine", "u-me") });
    expect(state.teamDialog).toBeNull();
  });

  it("a tombstone or delete drops the process", () => {
    let state = run(withTeam(), { type: "space", space: "t1" });
    state = run(state, { type: "team-page", teamId: "t1", page: { processes: [{ ...tp("p2", 6, 5), deleted: true }], lastSeq: 6, hasMore: false } });
    expect(recordOf(state, "p2")).toBeUndefined();
    state = run(state, { type: "team-removed", teamId: "t1", processId: "p1" });
    expect(spaceRecords(state)).toEqual([]);
  });

  it("merges fetched versions without moving a draft's base", () => {
    let state = run(withTeam(), { type: "space", space: "t1" });
    const versions = [1, 2, 3, 4].map((v) => ({ ...latest(recordOf(state, "p1")!), version: v }));
    state = run(state, { type: "team-versions", teamId: "t1", processId: "p1", versions });
    expect(recordOf(state, "p1")!.versions.map((v) => v.version)).toEqual([1, 2, 3]);
  });
});
