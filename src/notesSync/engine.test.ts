import { beforeEach, describe, expect, it, vi } from "vitest";
import { CloudError } from "../cloud/api";
import type { Change, ChangePage, Space, SyncDocument } from "../cloud/types";
import {
  NotesSyncEngine,
  type EngineClock,
  type EngineConfig,
  type IndexStore,
  type LocalFile,
  type NotesIO,
  type SyncApi,
} from "./engine";
import { byteLength, sha256Hex } from "./hash";
import { MAX_CONTENT_BYTES, pathProblem, type SpaceLayout } from "./paths";
import type { SpaceIndex } from "./plan";

/** A notes folder in memory. */
class MemoryFolder implements NotesIO {
  files = new Map<string, { content: string; mtimeMs: number }>();
  private clock = 1;

  async list(): Promise<LocalFile[]> {
    return [...this.files].map(([relPath, file]) => ({
      relPath,
      mtimeMs: file.mtimeMs,
      size: byteLength(file.content),
    }));
  }
  async read(relPath: string) {
    const file = this.files.get(relPath);
    if (!file) throw new Error(`${relPath} does not exist`);
    return file.content;
  }
  async write(relPath: string, content: string) {
    const mtimeMs = ++this.clock;
    this.files.set(relPath, { content, mtimeMs });
    return { mtimeMs };
  }
  async remove(relPath: string) {
    this.files.delete(relPath);
  }
  /** An edit made by the user (or the editor's autosave). */
  edit(relPath: string, content: string) {
    void this.write(relPath, content);
  }
}

/** One server space that follows the API's rules for revisions and paths. */
class FakeServer implements SyncApi {
  docs = new Map<string, SyncDocument>();
  feed: Change[] = [];
  offline = false;
  /** The next put is stored but its answer is lost. */
  dropNextAnswer = false;
  /** The next change feed call answers 410. */
  gone = false;
  /** The next change feed call with a cursor answers 400 "invalid cursor". */
  badCursor = false;
  /** Every call answers 404: the user left the team. */
  spaceGone = false;
  /** Answers to give instead, by method, once each. */
  failures: Record<string, CloudError[]> = {};
  calls = 0;
  puts = 0;

  fail(method: keyof SyncApi, error: CloudError) {
    (this.failures[method] ??= []).push(error);
  }
  private check(method: keyof SyncApi) {
    if (this.offline) throw new CloudError(0, "offline", "offline");
    this.calls++;
    if (this.spaceGone) {
      throw new CloudError(404, "not_found", method === "getTeamSpace" ? "team not found" : "space not found");
    }
    const failure = this.failures[method]?.shift();
    if (failure) throw failure;
  }
  async getPersonalSpace(): Promise<Space> {
    this.check("getPersonalSpace");
    return { id: "space-p", kind: "personal", lastSeq: 0 };
  }
  async getTeamSpace(teamId: string): Promise<Space> {
    this.check("getTeamSpace");
    return { id: `space-${teamId}`, kind: "team", teamId, lastSeq: 0 };
  }
  async listChanges(_spaceId: string, cursor: string): Promise<ChangePage> {
    this.check("listChanges");
    if (this.gone) {
      this.gone = false;
      throw new CloudError(410, "gone", "gone");
    }
    if (this.badCursor && cursor) {
      this.badCursor = false;
      throw new CloudError(400, "bad_request", "invalid cursor");
    }
    const from = cursor ? Number(cursor) : 0;
    return { changes: this.feed.slice(from), cursor: String(this.feed.length), hasMore: false };
  }
  async getDocuments(_spaceId: string, ids: string[]) {
    this.check("getDocuments");
    return ids.flatMap((id) => (this.docs.has(id) ? [this.docs.get(id)!] : []));
  }
  async putDocument(spaceId: string, id: string, path: string, content: string, baseRevision: number) {
    this.check("putDocument");
    this.puts++;
    const problem = pathProblem(spaceId === "space-p" ? "personal" : "team", path);
    if (problem) throw new CloudError(400, "bad_request", problem);
    if (byteLength(content) > MAX_CONTENT_BYTES) throw new CloudError(400, "bad_request", "content is larger than 1 MiB");
    const current = this.docs.get(id);
    const holder = [...this.docs.values()].find((d) => d.path === path && !d.deleted && d.id !== id);
    if (holder) throw new CloudError(409, "conflict", "path in use", holder);
    if (current && baseRevision === 0 && current.path === path && current.content === content) return current;
    if (!current && baseRevision !== 0) throw new CloudError(404, "not_found", "document not found");
    if (current && current.revision !== baseRevision) throw new CloudError(409, "conflict", "stale", current);
    const doc = await this.store(spaceId, id, path, content, (current?.revision ?? 0) + 1, "me");
    if (this.dropNextAnswer) {
      this.dropNextAnswer = false;
      throw new CloudError(0, "offline", "answer lost");
    }
    return doc;
  }
  async deleteDocument(_spaceId: string, id: string, baseRevision: number) {
    this.check("deleteDocument");
    const current = this.docs.get(id);
    if (!current) throw new CloudError(404, "not_found", "document not found");
    if (current.deleted || current.revision !== baseRevision) throw new CloudError(409, "conflict", "stale", current);
    const doc = { ...current, content: "", deleted: true, revision: current.revision + 1 };
    this.docs.set(id, doc);
    this.feed.push({ docId: id, path: doc.path, revision: doc.revision, deleted: true, sha256: doc.sha256 });
    return doc;
  }

  /** A save from someone else. */
  async store(spaceId: string, id: string, path: string, content: string, revision: number, by: string) {
    const doc: SyncDocument = {
      id,
      spaceId,
      path,
      content,
      sha256: await sha256Hex(content),
      revision,
      deleted: false,
      updatedBy: by,
      updatedAt: "2026-10-12T10:42:00.000Z",
    };
    this.docs.set(id, doc);
    this.feed.push({ docId: id, path, revision, deleted: false, sha256: doc.sha256 });
    return doc;
  }
  livePaths() {
    return [...this.docs.values()].filter((d) => !d.deleted).map((d) => d.path).sort();
  }
}

class MemoryStore implements IndexStore {
  saved = new Map<string, SpaceIndex>();
  load(userId: string, space: string) {
    const index = this.saved.get(`${userId}.${space}`);
    return index ? structuredClone(index) : null;
  }
  save(userId: string, space: string, index: SpaceIndex) {
    this.saved.set(`${userId}.${space}`, structuredClone(index));
  }
  remove(userId: string, space: string) {
    this.saved.delete(`${userId}.${space}`);
  }
}

/** Time that only moves when a test moves it. */
class TestClock implements EngineClock {
  at = 1_000_000;
  timers: { at: number; ms: number; run: () => void; cancelled: boolean }[] = [];
  now = () => this.at;
  after = (ms: number, run: () => void) => {
    const timer = { at: this.at + ms, ms, run, cancelled: false };
    this.timers.push(timer);
    return () => {
      timer.cancelled = true;
    };
  };
  pending() {
    return this.timers.filter((timer) => !timer.cancelled);
  }
  /** Moves time on and runs the timers that are due. */
  advance(ms: number) {
    this.at += ms;
    for (const timer of this.pending()) {
      if (timer.at <= this.at) {
        timer.cancelled = true;
        timer.run();
      }
    }
  }
}

const TEAM_LAYOUT: SpaceLayout = { spaces: [{ key: "t1", folder: "Northwind" }], teamFolders: ["Northwind"] };

describe("NotesSyncEngine", () => {
  let folder: MemoryFolder;
  let server: FakeServer;
  let store: MemoryStore;
  let engine: NotesSyncEngine;
  let editing: Set<string>;
  let ids: number;
  let clock: TestClock;
  let goneSpaces: string[];

  const config = (layout: SpaceLayout = TEAM_LAYOUT): EngineConfig => ({
    userId: "me",
    root: "/notes",
    layout,
    nameOf: async (_space, userId) => (userId === "maya" ? "Maya Chen" : "another device"),
    editing: (relPath) => editing.has(relPath),
    onSynced: () => {},
    onGone: (space) => goneSpaces.push(space),
  });

  beforeEach(() => {
    folder = new MemoryFolder();
    server = new FakeServer();
    store = new MemoryStore();
    editing = new Set();
    ids = 0;
    clock = new TestClock();
    goneSpaces = [];
    engine = new NotesSyncEngine(folder, server, store, {
      newId: () => `doc-${++ids}`,
      clock,
      pace: null,
    });
    engine.configure(config());
  });

  it("pushes a new note once", async () => {
    folder.edit("Northwind/agents/claude-code.md", "# Claude Code\n");
    await engine.sync(true);
    expect(server.livePaths()).toEqual(["agents/claude-code.md"]);
    await engine.sync(true);
    expect(server.puts).toBe(1);
    expect(engine.snapshot.waiting).toEqual([]);
  });

  it("leaves files outside the team folder and non-markdown alone", async () => {
    folder.edit("todo.md", "x");
    folder.edit("Northwind/diagram.png", "png");
    await engine.sync(true);
    expect(server.livePaths()).toEqual([]);
  });

  it("pulls a teammate's note into the team folder", async () => {
    await server.store("space-t1", "remote-1", "release-checklist.md", "- [ ] tag\n", 1, "maya");
    await engine.sync(true);
    expect(folder.files.get("Northwind/release-checklist.md")?.content).toBe("- [ ] tag\n");
  });

  it("follows edits, renames and deletes from the server", async () => {
    await server.store("space-t1", "r", "a.md", "one", 1, "maya");
    await engine.sync(true);
    await server.store("space-t1", "r", "b.md", "two", 2, "maya");
    await engine.sync(true);
    expect(folder.files.has("Northwind/a.md")).toBe(false);
    expect(folder.files.get("Northwind/b.md")?.content).toBe("two");
    await server.deleteDocument("space-t1", "r", 2);
    await engine.sync(true);
    expect(folder.files.has("Northwind/b.md")).toBe(false);
  });

  it("sends local renames and deletes", async () => {
    folder.edit("Northwind/a.md", "same");
    await engine.sync(true);
    await folder.remove("Northwind/a.md");
    folder.edit("Northwind/moved/a.md", "same");
    await engine.sync(true);
    expect(server.livePaths()).toEqual(["moved/a.md"]);
    expect(server.docs.size).toBe(1);
    await folder.remove("Northwind/moved/a.md");
    await engine.sync(true);
    expect(server.livePaths()).toEqual([]);
  });

  it("keeps both versions when both sides changed, then Keep mine wins", async () => {
    folder.edit("Northwind/agents/claude-code.md", "base");
    await engine.sync(true);
    const [doc] = server.docs.values();
    await server.store("space-t1", doc.id, doc.path, "theirs", doc.revision + 1, "maya");
    folder.edit("Northwind/agents/claude-code.md", "mine");

    await engine.sync(true);
    expect(folder.files.get("Northwind/agents/claude-code.md")?.content).toBe("mine");
    expect(folder.files.get("Northwind/agents/claude-code (Maya Chen).md")?.content).toBe("theirs");
    expect(engine.snapshot.conflicts).toMatchObject([
      {
        path: "Northwind/agents/claude-code.md",
        copyPath: "Northwind/agents/claude-code (Maya Chen).md",
        theirName: "Maya Chen",
      },
    ]);
    // Neither side pushes while the conflict is open.
    await engine.sync(true);
    expect(server.livePaths()).toEqual(["agents/claude-code.md"]);

    await engine.keepMine("t1", doc.id);
    expect(server.docs.get(doc.id)?.content).toBe("mine");
    expect(folder.files.has("Northwind/agents/claude-code (Maya Chen).md")).toBe(false);
    expect(engine.snapshot.conflicts).toEqual([]);
  });

  it("Use theirs replaces yours with their version", async () => {
    folder.edit("Northwind/a.md", "base");
    await engine.sync(true);
    const [doc] = server.docs.values();
    await server.store("space-t1", doc.id, doc.path, "theirs", doc.revision + 1, "maya");
    folder.edit("Northwind/a.md", "mine");
    await engine.sync(true);

    await engine.useTheirs("t1", doc.id, true);
    expect(folder.files.get("Northwind/a.md")?.content).toBe("theirs");
    expect(folder.files.has("Northwind/a (Maya Chen).md")).toBe(false);
    expect(server.docs.get(doc.id)?.content).toBe("theirs");
    expect(server.docs.get(doc.id)?.revision).toBe(doc.revision + 1);
    expect(engine.snapshot.conflicts).toEqual([]);
  });

  it("holds a pulled change while the note is open with unsaved edits", async () => {
    folder.edit("Northwind/a.md", "base");
    await engine.sync(true);
    const [doc] = server.docs.values();
    await server.store("space-t1", doc.id, doc.path, "theirs", doc.revision + 1, "maya");
    editing.add("Northwind/a.md");
    await engine.sync(true);
    expect(folder.files.get("Northwind/a.md")?.content).toBe("base");
    editing.clear();
    await engine.sync(true);
    expect(folder.files.get("Northwind/a.md")?.content).toBe("theirs");
  });

  it("queues changes while offline and sends them when back", async () => {
    folder.edit("Northwind/a.md", "a");
    folder.edit("Northwind/b.md", "b");
    server.offline = true;
    await engine.sync(true);
    expect(engine.snapshot.offline).toBe(true);
    expect(engine.snapshot.waiting.sort()).toEqual(["Northwind/a.md", "Northwind/b.md"]);
    expect(engine.snapshot.waitingCount).toBe(2);

    server.offline = false;
    await engine.sync(true);
    expect(engine.snapshot.offline).toBe(false);
    expect(engine.snapshot.waitingCount).toBe(0);
    expect(server.livePaths()).toEqual(["a.md", "b.md"]);
  });

  it("retries a create whose answer was lost with the same id", async () => {
    folder.edit("Northwind/a.md", "a");
    server.dropNextAnswer = true;
    await engine.sync(true);
    await engine.sync(true);
    expect(server.docs.size).toBe(1);
    expect(engine.snapshot.waitingCount).toBe(0);
  });

  it("starts the change feed over after a 410", async () => {
    await server.store("space-t1", "r", "a.md", "one", 1, "maya");
    await engine.sync(true);
    await folder.remove("Northwind/a.md");
    // The local delete pushes, and the feed restarts cleanly.
    server.gone = true;
    await engine.sync(true);
    expect(server.livePaths()).toEqual([]);
  });

  it("keeps a different local note that sits where a remote one lands", async () => {
    engine.configure(config({ spaces: [{ key: "personal", folder: "" }], teamFolders: [] }));
    folder.edit("todo.md", "this computer");
    await server.store("space-p", "r", "todo.md", "laptop", 1, "me");
    await engine.sync(true);
    expect(folder.files.get("todo.md")?.content).toBe("this computer");
    expect(folder.files.get("todo (another device).md")?.content).toBe("laptop");
    expect(server.docs.size).toBe(1);
  });

  describe("the server's rules", () => {
    it("never sends a note whose name the server would refuse, and says why", async () => {
      folder.edit("Northwind/a\\b.md", "backslash");
      folder.edit(`Northwind/${"n".repeat(510)}.md`, "long");
      folder.edit("Northwind/tab\there.md", "control");
      folder.edit("Northwind/ok.md", "fine");
      await engine.sync(true);
      expect(server.puts).toBe(1);
      expect(server.livePaths()).toEqual(["ok.md"]);
      expect(engine.snapshot.reasons).toEqual({
        "Northwind/a\\b.md": "Stays on this computer: path has a backslash.",
        [`Northwind/${"n".repeat(510)}.md`]: "Stays on this computer: path is longer than 512 bytes.",
        "Northwind/tab\there.md": "Stays on this computer: path has a control character.",
      });
      expect(engine.snapshot.refused).toHaveLength(3);
      expect(engine.snapshot.waiting).toEqual([]);
    });

    it("leaves .MD alone: extensions are case sensitive", async () => {
      folder.edit("Northwind/Plan.MD", "x");
      await engine.sync(true);
      expect(server.puts).toBe(0);
      expect(engine.snapshot.waitingCount).toBe(0);
    });

    it("keeps a note over 1 MiB here, without deleting the synced version", async () => {
      folder.edit("Northwind/big.md", "small");
      folder.edit("Northwind/huge.md", "x".repeat(MAX_CONTENT_BYTES + 1));
      await engine.sync(true);
      expect(server.livePaths()).toEqual(["big.md"]);
      folder.edit("Northwind/big.md", "é".repeat(MAX_CONTENT_BYTES / 2 + 1));
      await engine.sync(true);
      expect(server.puts).toBe(1);
      expect(server.livePaths()).toEqual(["big.md"]);
      expect([...server.docs.values()][0].content).toBe("small");
      expect(engine.snapshot.reasons).toEqual({
        "Northwind/big.md": "Stays on this computer: content is larger than 1 MiB.",
        "Northwind/huge.md": "Stays on this computer: content is larger than 1 MiB.",
      });
      // Small again: it syncs.
      folder.edit("Northwind/big.md", "small again");
      await engine.sync(true);
      expect([...server.docs.values()][0].content).toBe("small again");
      expect(engine.snapshot.refused).toEqual(["Northwind/huge.md"]);
    });

    it("keeps the server's reason for a refusal, and sends again once the note changes", async () => {
      folder.edit("Northwind/a.md", "a");
      server.fail("putDocument", new CloudError(400, "bad_request", "this space already has 5000 documents"));
      await engine.sync(true);
      expect(engine.snapshot.reasons).toEqual({
        "Northwind/a.md": "Stays on this computer: this space already has 5000 documents.",
      });
      // Not sent again as it is (only the change feed is read).
      const calls = server.calls;
      await engine.sync(true);
      expect(server.calls).toBe(calls + 1);
      expect(engine.snapshot.waitingCount).toBe(0);
      folder.edit("Northwind/a.md", "a, edited");
      await engine.sync(true);
      expect(server.livePaths()).toEqual(["a.md"]);
      expect(engine.snapshot.refused).toEqual([]);
    });

    it("sends a note again as new when the server has no such document", async () => {
      folder.edit("Northwind/a.md", "a");
      await engine.sync(true);
      server.docs.clear();
      folder.edit("Northwind/a.md", "a, edited");
      await engine.sync(true);
      await engine.sync(true);
      expect(server.livePaths()).toEqual(["a.md"]);
      expect(engine.snapshot.waitingCount).toBe(0);
      expect(engine.snapshot.refused).toEqual([]);
    });
  });

  describe("try again later", () => {
    it("treats 429 as waiting, not refused, and retries after a wait", async () => {
      folder.edit("Northwind/a.md", "a");
      server.fail("putDocument", new CloudError(429, "rate_limited", "too many requests, retry later"));
      await engine.sync(true);
      expect(engine.snapshot).toMatchObject({
        refused: [],
        offline: false,
        retrying: true,
        waiting: ["Northwind/a.md"],
      });
      expect(clock.pending().map((timer) => timer.ms)).toEqual([2_000]);

      // A round before the wait is up leaves the server alone.
      const calls = server.calls;
      await engine.sync(true);
      expect(server.calls).toBe(calls);

      clock.advance(2_000);
      await engine.sync(true); // joins the round the timer started
      expect(server.livePaths()).toEqual(["a.md"]);
      expect(engine.snapshot.retrying).toBe(false);
      expect(engine.snapshot.waiting).toEqual([]);
      expect(clock.pending()).toEqual([]);
    });

    it("waits longer each time the server says no in a row", async () => {
      folder.edit("Northwind/a.md", "a");
      const busy = () => new CloudError(429, "rate_limited", "too many requests, retry later");
      server.fail("listChanges", busy());
      server.fail("listChanges", busy());
      await engine.sync(true);
      clock.advance(2_000);
      await engine.sync(true);
      expect(clock.pending().map((timer) => timer.ms)).toEqual([4_000]);
      clock.advance(4_000);
      await engine.sync(true);
      expect(server.livePaths()).toEqual(["a.md"]);
      expect(engine.snapshot.retrying).toBe(false);
    });

    it("stops the round on a 408 too, and the other spaces wait with it", async () => {
      engine.configure(
        config({
          spaces: [
            { key: "personal", folder: "" },
            { key: "t1", folder: "Northwind" },
          ],
          teamFolders: ["Northwind"],
        }),
      );
      folder.edit("todo.md", "mine");
      folder.edit("Northwind/a.md", "team");
      server.fail("getPersonalSpace", new CloudError(408, "timeout", "request timeout"));
      await engine.sync(true);
      expect(server.calls).toBe(1);
      expect(engine.snapshot.waitingCount).toBe(2);
      expect(engine.snapshot.retrying).toBe(true);
      clock.advance(2_000);
      await engine.sync(true);
      expect(engine.snapshot.waitingCount).toBe(0);
    });
  });

  describe("the change feed", () => {
    it("starts over when the server doesn't take the cursor (400)", async () => {
      await server.store("space-t1", "r1", "a.md", "one", 1, "maya");
      await engine.sync(true);
      await server.store("space-t1", "r2", "b.md", "two", 1, "maya");
      server.badCursor = true;
      await engine.sync(true);
      expect(folder.files.get("Northwind/b.md")?.content).toBe("two");
      expect(folder.files.get("Northwind/a.md")?.content).toBe("one");
      expect(store.saved.get("me.t1")?.cursor).toBe("2");
    });

    it("stops syncing a space that is gone (404) and drops its index", async () => {
      const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
      folder.edit("Northwind/a.md", "a");
      await engine.sync(true);
      expect(store.saved.has("me.t1")).toBe(true);

      server.spaceGone = true;
      folder.edit("Northwind/a.md", "edited after leaving");
      await engine.sync(true);
      expect(goneSpaces).toEqual(["t1"]);
      expect(store.saved.has("me.t1")).toBe(false);
      expect(engine.snapshot.waiting).toEqual([]);
      expect(engine.snapshot.offline).toBe(false);
      // The notes stay in the folder.
      expect(folder.files.get("Northwind/a.md")?.content).toBe("edited after leaving");

      const calls = server.calls;
      await engine.sync(true);
      expect(server.calls).toBe(calls);
      expect(warn).not.toHaveBeenCalled();
      warn.mockRestore();
    });

    it("drops a team space whose team answers 404 before its first sync", async () => {
      server.spaceGone = true;
      folder.edit("Northwind/a.md", "a");
      await engine.sync(true);
      expect(goneSpaces).toEqual(["t1"]);
      expect(store.saved.size).toBe(0);
    });
  });
});
