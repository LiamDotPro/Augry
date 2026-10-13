import { describe, expect, it, vi } from "vitest";
import type { ProcessPage, TeamProcess } from "../cloud/types";
import { CONNECTIONS } from "./connections";
import { BUG_FIX, FEATURE_DELIVERY, CRASH_REPORT } from "./examples";
import { cloneProcess } from "./json";
import { connectionsUsed, connectionRefs } from "./share";
import {
  applyPage,
  definitionToProcess,
  EMPTY_TEAM_CACHE,
  firstName,
  memberNames,
  putProcess,
  readFeed,
  savedAgo,
  savedLine,
  syncTeamRecords,
} from "./team";
import { latest, withDraft } from "./versions";

const HOUR = 3600000;

function tp(id: string, seq: number, version = 1, extra: Partial<TeamProcess> = {}): TeamProcess {
  return {
    id,
    teamId: "t1",
    name: "Bug fix",
    version,
    seq,
    deleted: false,
    updatedBy: "u-maya",
    updatedAt: new Date(Date.UTC(2026, 9, 12, 10, seq)).toISOString(),
    definition: { ...cloneProcess(BUG_FIX), id: "bug-fix" },
    ...extra,
  };
}

describe("applyPage", () => {
  it("adds, replaces and drops tombstones", () => {
    let cache = applyPage(EMPTY_TEAM_CACHE, { processes: [tp("a", 1), tp("b", 2)], lastSeq: 2, hasMore: false });
    cache = applyPage(cache, { processes: [tp("a", 3, 2), tp("b", 4, 1, { deleted: true })], lastSeq: 4, hasMore: false });
    expect(Object.keys(cache.processes)).toEqual(["a"]);
    expect(cache.processes.a!.version).toBe(2);
    expect(cache.lastSeq).toBe(4);
  });

  it("moves the cursor only to the last item while more pages follow", () => {
    const cache = applyPage(EMPTY_TEAM_CACHE, { processes: [tp("a", 1), tp("b", 2)], lastSeq: 9, hasMore: true });
    expect(cache.lastSeq).toBe(2);
  });

  it("keeps a newer local write over an older cached one", () => {
    const cache = putProcess(applyPage(EMPTY_TEAM_CACHE, { processes: [tp("a", 1, 3)], lastSeq: 1, hasMore: false }), tp("a", 1, 2));
    expect(cache.processes.a!.version).toBe(3);
  });
});

describe("readFeed", () => {
  it("pages with the last item's seq until hasMore is false", async () => {
    const pages: ProcessPage[] = [
      { processes: [tp("a", 1), tp("b", 2)], lastSeq: 3, hasMore: true },
      { processes: [tp("c", 3)], lastSeq: 3, hasMore: false },
    ];
    const list = vi.fn(async (_since: number) => pages.shift()!);
    let cache = EMPTY_TEAM_CACHE;
    await readFeed(list, 0, (page) => (cache = applyPage(cache, page)));
    expect(list.mock.calls.map((c) => c[0])).toEqual([0, 2]);
    expect(Object.keys(cache.processes).sort()).toEqual(["a", "b", "c"]);
    expect(cache.lastSeq).toBe(3);
  });

  it("stops when a page doesn't move on", async () => {
    const list = vi.fn(async () => ({ processes: [tp("a", 1)], lastSeq: 1, hasMore: true }));
    await readFeed(list, 1, () => {});
    expect(list).toHaveBeenCalledTimes(1);
  });
});

describe("definitionToProcess", () => {
  it("numbers the process with the server's version", () => {
    const p = definitionToProcess({ ...cloneProcess(BUG_FIX), version: 1 }, 4, "x");
    expect(p?.version).toBe(4);
    expect(p?.id).toBe("bug-fix");
  });

  it("refuses something that isn't a process", () => {
    expect(definitionToProcess({ name: "nope" }, 1, "x")).toBeNull();
    expect(definitionToProcess("text", 1, "x")).toBeNull();
  });
});

describe("syncTeamRecords", () => {
  it("makes a record per live process, newest save first", () => {
    const cache = applyPage(EMPTY_TEAM_CACHE, { processes: [tp("a", 1), tp("b", 2, 3)], lastSeq: 2, hasMore: false });
    const records = syncTeamRecords([], cache);
    expect(records.map((r) => r.id)).toEqual(["b", "a"]);
    expect(latest(records[0]!).version).toBe(3);
    expect(latest(records[0]!).byId).toBe("u-maya");
  });

  it("keeps a draft on the version it started from", () => {
    let cache = applyPage(EMPTY_TEAM_CACHE, { processes: [tp("a", 1, 3)], lastSeq: 1, hasMore: false });
    const [record] = syncTeamRecords([], cache);
    const edited = withDraft(record!, { ...latest(record!).process, name: "Mine" });
    cache = applyPage(cache, { processes: [tp("a", 2, 4)], lastSeq: 2, hasMore: false });
    const [kept] = syncTeamRecords([edited], cache);
    expect(latest(kept!).version).toBe(3);
    expect(kept!.draft?.name).toBe("Mine");
  });

  it("adds the new version to a record without a draft and skips broken files", () => {
    let cache = applyPage(EMPTY_TEAM_CACHE, { processes: [tp("a", 1, 3)], lastSeq: 1, hasMore: false });
    const first = syncTeamRecords([], cache);
    cache = applyPage(cache, { processes: [tp("a", 2, 4), tp("z", 3, 1, { definition: { broken: true } })], lastSeq: 3, hasMore: false });
    const [record, ...rest] = syncTeamRecords(first, cache);
    expect(record!.versions.map((v) => v.version)).toEqual([3, 4]);
    expect(rest).toEqual([]);
  });
});

describe("names and times", () => {
  it("names members by name, else login", () => {
    const names = memberNames([
      { user: { id: "1", login: "maya", name: "Maya Chen", avatarUrl: "" }, role: "owner", joinedAt: "" },
      { user: { id: "2", login: "sam", name: " ", avatarUrl: "" }, role: "member", joinedAt: "" },
    ]);
    expect(names).toEqual({ "1": "Maya Chen", "2": "sam" });
    expect(firstName("Maya Chen")).toBe("Maya");
  });

  it("says when", () => {
    const now = new Date(2026, 9, 12, 12, 0).getTime();
    expect(savedAgo(now - 30000, now)).toBe("just now");
    expect(savedAgo(now - 5 * 60000, now)).toBe("5m ago");
    expect(savedLine("Maya Chen", 3, now - 2 * HOUR, now)).toBe("Maya Chen saved v3 · 2h ago");
    expect(savedAgo(now - 24 * HOUR, now)).toBe("yesterday");
    expect(savedAgo(now - 3 * 24 * HOUR, now)).toBe("3 days ago");
  });
});

describe("connectionsUsed", () => {
  const local = CONNECTIONS;

  it("finds the tracker a ticket start reads and gh commands", () => {
    expect(connectionsUsed(FEATURE_DELIVERY, local).map((c) => c.label)).toEqual(["GitHub Issues"]);
    expect(connectionsUsed(BUG_FIX, local).map((c) => c.label)).toEqual(["GitHub Issues"]);
  });

  it("reads ${conn:name} references and leaves local ones out", () => {
    const p = cloneProcess(CRASH_REPORT);
    p.steps[0]!.prompt = "Look up the user in ${conn:postgresql.staging} and ${conn:sentry}";
    p.steps[1]!.connection = "folder";
    expect(connectionRefs(p)).toContain("folder");
    const used = connectionsUsed(p, local);
    expect(used.map((c) => c.label)).toEqual(["GitHub Issues", "PostgreSQL · staging", "sentry"]);
    expect(used[1]!.kind).toBe("postgresql");
    expect(used[2]!.local).toBeUndefined();
  });
});
