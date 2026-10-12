import { describe, expect, it } from "vitest";
import type { Change } from "../cloud/types";
import { decidePull, latestChanges, planPush, waitingCount, waitingPaths, type IndexEntry, type PullContext } from "./plan";

const change = (over: Partial<Change> = {}): Change => ({
  docId: "d1",
  path: "a.md",
  revision: 2,
  deleted: false,
  sha256: "new",
  ...over,
});

const ctx = (over: Partial<PullContext> & { files?: Record<string, string> } = {}): PullContext => {
  const files = over.files ?? {};
  return {
    entry: undefined,
    localSha: (path) => files[path],
    editing: () => false,
    inConflict: false,
    accepts: true,
    ...over,
  };
};

const entry = (over: Partial<IndexEntry> = {}): IndexEntry => ({ path: "a.md", revision: 1, sha256: "old", ...over });

describe("decidePull", () => {
  it("writes a new remote note", () => {
    expect(decidePull(change(), ctx())).toEqual({ kind: "write" });
  });

  it("writes over a note that didn't change here", () => {
    expect(decidePull(change(), ctx({ entry: entry(), files: { "a.md": "old" } })).kind).toBe("write");
  });

  it("never overwrites a note that changed here since the last sync", () => {
    expect(decidePull(change(), ctx({ entry: entry(), files: { "a.md": "mine" } }))).toEqual({ kind: "keep-local" });
  });

  it("waits while the note is open with unsaved edits", () => {
    expect(
      decidePull(change(), ctx({ entry: entry(), files: { "a.md": "old" }, editing: (p) => p === "a.md" })),
    ).toEqual({ kind: "defer" });
  });

  it("skips its own write coming back", () => {
    expect(decidePull(change({ revision: 1 }), ctx({ entry: entry(), files: { "a.md": "old" } }))).toEqual({
      kind: "skip",
    });
  });

  it("only records the revision when the content already matches", () => {
    expect(decidePull(change(), ctx({ files: { "a.md": "new" } }))).toEqual({ kind: "adopt" });
  });

  it("keeps both when a different local note already has that path", () => {
    expect(decidePull(change(), ctx({ files: { "a.md": "other" } }))).toEqual({ kind: "conflict" });
  });

  it("moves a renamed note", () => {
    expect(
      decidePull(change({ path: "b.md" }), ctx({ entry: entry(), files: { "a.md": "old" } })),
    ).toEqual({ kind: "write", removeOld: "a.md" });
  });

  it("deletes a note deleted there and untouched here", () => {
    expect(decidePull(change({ deleted: true }), ctx({ entry: entry(), files: { "a.md": "old" } }))).toEqual({
      kind: "delete-local",
      path: "a.md",
    });
  });

  it("keeps a note deleted there but edited here", () => {
    expect(decidePull(change({ deleted: true }), ctx({ entry: entry(), files: { "a.md": "mine" } }))).toEqual({
      kind: "forget",
    });
  });

  it("refreshes the kept copy of a note already in conflict", () => {
    expect(decidePull(change(), ctx({ entry: entry(), inConflict: true }))).toEqual({ kind: "refresh-conflict" });
  });

  it("ignores documents this device doesn't hold", () => {
    expect(decidePull(change({ path: "settings.json" }), ctx({ accepts: false }))).toEqual({ kind: "skip" });
  });
});

describe("latestChanges", () => {
  it("keeps the last change per document, in feed order", () => {
    const list = latestChanges([
      change({ docId: "a", revision: 1 }),
      change({ docId: "b", revision: 1 }),
      change({ docId: "a", revision: 2 }),
    ]);
    expect(list.map((c) => `${c.docId}${c.revision}`)).toEqual(["b1", "a2"]);
  });
});

describe("planPush", () => {
  let n = 0;
  const newId = () => `new-${++n}`;

  it("creates new notes with baseRevision 0", () => {
    n = 0;
    expect(planPush({ docs: {} }, { "a.md": "x" }, new Set(), newId)).toEqual([
      { kind: "put", docId: "new-1", path: "a.md", baseRevision: 0 },
    ]);
  });

  it("updates changed notes from the revision last seen", () => {
    const ops = planPush({ docs: { d1: entry() } }, { "a.md": "mine" }, new Set(), newId);
    expect(ops).toEqual([{ kind: "put", docId: "d1", path: "a.md", baseRevision: 1 }]);
  });

  it("leaves unchanged notes alone", () => {
    expect(planPush({ docs: { d1: entry() } }, { "a.md": "old" }, new Set(), newId)).toEqual([]);
  });

  it("deletes notes deleted here", () => {
    expect(planPush({ docs: { d1: entry() } }, {}, new Set(), newId)).toEqual([
      { kind: "delete", docId: "d1", path: "a.md", baseRevision: 1 },
    ]);
  });

  it("sends a rename as a put with the new path", () => {
    expect(planPush({ docs: { d1: entry() } }, { "b.md": "old" }, new Set(), newId)).toEqual([
      { kind: "put", docId: "d1", path: "b.md", baseRevision: 1, fromPath: "a.md" },
    ]);
  });

  it("retries a create that never answered with the same id", () => {
    const docs = { pending: entry({ revision: 0, sha256: "" }) };
    expect(planPush({ docs }, { "a.md": "x" }, new Set(), newId)).toEqual([
      { kind: "put", docId: "pending", path: "a.md", baseRevision: 0 },
    ]);
    expect(planPush({ docs }, {}, new Set(), newId)).toEqual([{ kind: "forget", docId: "pending" }]);
  });

  it("holds back both sides of a conflict", () => {
    const ops = planPush(
      { docs: { d1: entry() } },
      { "a.md": "mine", "a (Maya).md": "theirs" },
      new Set(["a.md", "a (Maya).md"]),
      newId,
    );
    expect(ops).toEqual([]);
  });

  it("counts what waits", () => {
    const ops = planPush({ docs: { d1: entry(), d2: entry({ path: "gone.md" }) } }, { "a.md": "mine" }, new Set(), newId);
    expect(waitingPaths(ops)).toEqual(["a.md"]);
    expect(waitingCount(ops)).toBe(2);
  });
});
