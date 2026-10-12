import { beforeEach, describe, expect, it } from "vitest";
import { CloudError } from "../cloud/api";
import type { Change, ChangePage, Space, SyncDocument } from "../cloud/types";
import { NotesSyncEngine, type EngineConfig, type IndexStore, type LocalFile, type NotesIO, type SyncApi } from "./engine";
import { byteLength, sha256Hex } from "./hash";
import type { SpaceLayout } from "./paths";
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
  puts = 0;

  private check() {
    if (this.offline) throw new CloudError(0, "offline", "offline");
  }
  async getPersonalSpace(): Promise<Space> {
    this.check();
    return { id: "space-p", kind: "personal", lastSeq: 0 };
  }
  async getTeamSpace(teamId: string): Promise<Space> {
    this.check();
    return { id: `space-${teamId}`, kind: "team", teamId, lastSeq: 0 };
  }
  async listChanges(_spaceId: string, cursor: string): Promise<ChangePage> {
    this.check();
    if (this.gone) {
      this.gone = false;
      throw new CloudError(410, "gone", "gone");
    }
    const from = cursor ? Number(cursor) : 0;
    return { changes: this.feed.slice(from), cursor: String(this.feed.length), hasMore: false };
  }
  async getDocuments(_spaceId: string, ids: string[]) {
    this.check();
    return ids.flatMap((id) => (this.docs.has(id) ? [this.docs.get(id)!] : []));
  }
  async putDocument(spaceId: string, id: string, path: string, content: string, baseRevision: number) {
    this.check();
    this.puts++;
    const current = this.docs.get(id);
    const holder = [...this.docs.values()].find((d) => d.path === path && !d.deleted && d.id !== id);
    if (holder) throw new CloudError(409, "conflict", "path in use", holder);
    if (current && baseRevision === 0 && current.path === path && current.content === content) return current;
    if (!current && baseRevision !== 0) throw new CloudError(404, "not_found", "no such document");
    if (current && current.revision !== baseRevision) throw new CloudError(409, "conflict", "stale", current);
    const doc = await this.store(spaceId, id, path, content, (current?.revision ?? 0) + 1, "me");
    if (this.dropNextAnswer) {
      this.dropNextAnswer = false;
      throw new CloudError(0, "offline", "answer lost");
    }
    return doc;
  }
  async deleteDocument(_spaceId: string, id: string, baseRevision: number) {
    this.check();
    const current = this.docs.get(id);
    if (!current) throw new CloudError(404, "not_found", "no such document");
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
}

const TEAM_LAYOUT: SpaceLayout = { spaces: [{ key: "t1", folder: "Northwind" }], teamFolders: ["Northwind"] };

describe("NotesSyncEngine", () => {
  let folder: MemoryFolder;
  let server: FakeServer;
  let store: MemoryStore;
  let engine: NotesSyncEngine;
  let editing: Set<string>;
  let ids: number;

  const config = (layout: SpaceLayout = TEAM_LAYOUT): EngineConfig => ({
    userId: "me",
    root: "/notes",
    layout,
    nameOf: async (_space, userId) => (userId === "maya" ? "Maya Chen" : "another device"),
    editing: (relPath) => editing.has(relPath),
    onSynced: () => {},
  });

  beforeEach(() => {
    folder = new MemoryFolder();
    server = new FakeServer();
    store = new MemoryStore();
    editing = new Set();
    ids = 0;
    engine = new NotesSyncEngine(folder, server, store, () => `doc-${++ids}`);
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
});
