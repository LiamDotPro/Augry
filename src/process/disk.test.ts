import { describe, expect, it } from "vitest";
import { diskChanges, recordsFromDisk, type StoredProcess } from "./disk";
import { BUG_FIX } from "./examples";
import { hashOf } from "./json";
import type { ProcessRecord } from "./types";
import { saveVersion, withDraft } from "./versions";

const v1 = { version: 1, message: "Created", by: "You", at: 5, hash: hashOf(BUG_FIX), process: { ...BUG_FIX, version: 1 } };
const stored = (patch: Partial<StoredProcess>): StoredProcess => ({
  id: "bug-fix",
  versions: [v1],
  draft: null,
  file: v1.process,
  problem: null,
  ...patch,
});

describe("recordsFromDisk", () => {
  it("reads saved versions, oldest first", () => {
    const v2 = { ...v1, version: 2, message: "Second", process: { ...BUG_FIX, name: "Bug fix 2", version: 2 } };
    const { records, problems } = recordsFromDisk([stored({ versions: [v2, v1], file: v2.process })], 9);
    expect(problems).toEqual([]);
    expect(records[0]!.versions.map((v) => v.version)).toEqual([1, 2]);
    expect(records[0]!.draft).toBeNull();
  });

  it("keeps a draft, and shows a file edited outside the app as one", () => {
    const draft = { ...BUG_FIX, name: "Draft" };
    expect(recordsFromDisk([stored({ draft })], 9).records[0]!.draft?.name).toBe("Draft");
    const edited = { ...BUG_FIX, name: "Edited by hand", version: 1 };
    expect(recordsFromDisk([stored({ file: edited })], 9).records[0]!.draft?.name).toBe("Edited by hand");
  });

  it("starts a hand-written file at version 1", () => {
    const { records } = recordsFromDisk([stored({ versions: [], file: BUG_FIX })], 9);
    expect(records[0]!.versions).toHaveLength(1);
    expect(records[0]!.versions[0]!.message).toBe("Found in the processes folder");
    expect(records[0]!.versions[0]!.at).toBe(9);
  });

  it("leaves out what can't be read and says why", () => {
    const { records, problems } = recordsFromDisk(
      [stored({ id: "broken", versions: [{ nope: true }], file: { name: "x" }, problem: "broken.json: bad" })],
      9,
    );
    expect(records).toEqual([]);
    expect(problems).toEqual([{ id: "broken", message: "broken.json: bad" }]);
  });
});

describe("diskChanges", () => {
  const record: ProcessRecord = { id: "bug-fix", versions: [v1], draft: null };

  it("saves new records and new versions, and nothing for untouched ones", () => {
    expect(diskChanges([], [record]).saves.map((s) => s.version.version)).toEqual([1]);
    expect(diskChanges([record], [record])).toEqual({ saves: [], drafts: [], deletes: [] });
    const next = saveVersion(withDraft(record, { ...BUG_FIX, name: "Renamed" }), "Renamed", 10);
    const changes = diskChanges([record], [next]);
    expect(changes.saves.map((s) => s.version.version)).toEqual([2]);
    expect(changes.drafts).toEqual([]);
  });

  it("writes and clears drafts", () => {
    const drafted = withDraft(record, { ...BUG_FIX, name: "Draft" });
    expect(diskChanges([record], [drafted]).drafts).toEqual([{ id: "bug-fix", draft: drafted.draft }]);
    expect(diskChanges([drafted], [record]).drafts).toEqual([{ id: "bug-fix", draft: null }]);
  });

  it("deletes records that are gone", () => {
    expect(diskChanges([record], []).deletes).toEqual(["bug-fix"]);
  });
});
