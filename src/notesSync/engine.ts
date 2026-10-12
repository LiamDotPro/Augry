/**
 * Notes sync, the part that does IO (Notes board 02, "Sync engine notes").
 * Notes always save to disk first; a round runs in the background and never
 * blocks editing:
 *
 *   1. hash the local files (cached by mtime and size),
 *   2. pull: read the change feed from the space's cursor and apply it,
 *      never overwriting a file that changed here since the last sync,
 *   3. push: send what changed here, with the base revision this device last
 *      saw, and turn a 409 into a conflict copy the user picks from.
 *
 * Decisions are in plan.ts; this file only carries them out. Rounds, Keep
 * mine and Use theirs run one at a time.
 */
import { isCloudError } from "../cloud/api";
import type { Change, ChangePage, Space, SyncDocument } from "../cloud/types";
import { byteLength, sha256Hex } from "./hash";
import { localOfSpace, spaceOfLocal, conflictCopyPath, type SpaceKey, type SpaceLayout } from "./paths";
import {
  decidePull,
  emptyIndex,
  latestChanges,
  planPush,
  waitingCount,
  waitingPaths,
  type LocalHashes,
  type PullDecision,
  type SpaceIndex,
  type SyncConflict,
} from "./plan";

/** A file in the notes folder, as notes_tree lists it. */
export interface LocalFile {
  relPath: string;
  mtimeMs: number;
  size: number;
}

/** The notes folder (src-tauri notes commands). */
export interface NotesIO {
  list(): Promise<LocalFile[]>;
  read(relPath: string): Promise<string>;
  write(relPath: string, content: string): Promise<{ mtimeMs: number }>;
  /** To the bin, never a hard delete. */
  remove(relPath: string): Promise<void>;
}

/** The cloud calls sync uses (src/cloud/api.ts). */
export interface SyncApi {
  getPersonalSpace(): Promise<Space>;
  getTeamSpace(teamId: string): Promise<Space>;
  listChanges(spaceId: string, cursor: string): Promise<ChangePage>;
  getDocuments(spaceId: string, ids: string[]): Promise<SyncDocument[]>;
  putDocument(spaceId: string, docId: string, path: string, content: string, baseRevision: number): Promise<SyncDocument>;
  deleteDocument(spaceId: string, docId: string, baseRevision: number): Promise<SyncDocument>;
}

export interface IndexStore {
  load(userId: string, space: SpaceKey): SpaceIndex | null;
  save(userId: string, space: SpaceKey, index: SpaceIndex): void;
}

export interface EngineConfig {
  userId: string;
  /** The notes folder, absolute. */
  root: string;
  layout: SpaceLayout;
  /** Who saved a document, for the conflict copy's name. */
  nameOf(space: SpaceKey, userId: string): Promise<string>;
  /** True while that note (relative to the notes folder) is open with unsaved edits. */
  editing(relPath: string): boolean;
  /** A space finished a round with the server. */
  onSynced(space: SpaceKey, at: number): void;
}

/** A conflict, with paths relative to the notes folder. */
export interface LocalConflict {
  space: SpaceKey;
  docId: string;
  path: string;
  copyPath: string;
  theirName: string;
}

export interface SyncSnapshot {
  /** Notes with a change that hasn't reached the server, relative to the notes folder. */
  waiting: string[];
  waitingCount: number;
  conflicts: LocalConflict[];
  /** Notes the server refused (too big, a bad path): they stay on this computer. */
  refused: string[];
  /** The last round could not reach the server. */
  offline: boolean;
  /** When the last round that reached every space ended, epoch ms. */
  syncedAt: number | null;
}

export const EMPTY_SNAPSHOT: SyncSnapshot = {
  waiting: [],
  waitingCount: 0,
  conflicts: [],
  refused: [],
  offline: false,
  syncedAt: null,
};

/** GET …/documents takes at most 100 ids. */
const PAGE_IDS = 100;

/** Stops the round: the server is unreachable or this device was signed out. */
class Abort extends Error {}

interface SpaceState {
  index: SpaceIndex | null;
  /** Paths the server refused, with the content hash it refused. */
  refused: Map<string, string>;
  waiting: string[];
  waitingCount: number;
}

export class NotesSyncEngine {
  private config: EngineConfig | null = null;
  private spaces = new Map<SpaceKey, SpaceState>();
  private hashes = new Map<string, { mtimeMs: number; size: number; sha: string }>();
  private chain: Promise<unknown> = Promise.resolve();
  private queued: Promise<void> | null = null;
  private listeners = new Set<(snapshot: SyncSnapshot) => void>();
  private offline = false;
  private syncedAt: number | null = null;
  snapshot: SyncSnapshot = EMPTY_SNAPSHOT;

  constructor(
    private readonly io: NotesIO,
    private readonly api: SyncApi,
    private readonly store: IndexStore,
    private readonly newId: () => string = () => crypto.randomUUID(),
  ) {}

  /** A new account, folder or set of spaces: what was cached for the old one is dropped. */
  configure(config: EngineConfig | null): void {
    const before = this.config;
    this.config = config;
    if (!config || !before || before.userId !== config.userId || before.root !== config.root) {
      this.spaces.clear();
      this.hashes.clear();
      this.syncedAt = null;
    } else {
      const keep = new Set(config.layout.spaces.map((space) => space.key));
      for (const key of [...this.spaces.keys()]) if (!keep.has(key)) this.spaces.delete(key);
    }
    this.emit();
  }

  subscribe(listener: (snapshot: SyncSnapshot) => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  /**
   * One round over every synced space. Offline, it only works out what is
   * waiting. Calls made while a round is queued join that round.
   */
  sync(online: boolean): Promise<void> {
    if (this.queued) return this.queued;
    const queued = this.exclusive(async () => {
      this.queued = null;
      await this.round(online);
    });
    this.queued = queued;
    return queued;
  }

  /** Keep mine: save yours over theirs, then drop their copy. */
  keepMine(space: SpaceKey, docId: string): Promise<void> {
    return this.exclusive(async () => {
      const config = this.requireConfig();
      const state = await this.ensure(space, true);
      const index = state.index!;
      const conflict = index.conflicts.find((c) => c.docId === docId);
      if (!conflict) return;
      const local = this.toLocal(space, conflict.path);
      const copy = this.toLocal(space, conflict.copyPath);
      if (!local || !copy) return;
      const content = await this.io.read(local);
      const sha = await sha256Hex(content);
      try {
        const doc = await this.api.putDocument(index.spaceId, docId, conflict.path, content, conflict.revision);
        index.docs[docId] = { path: doc.path, revision: doc.revision, sha256: sha };
        index.conflicts = index.conflicts.filter((c) => c !== conflict);
        await this.io.remove(copy).catch(() => {});
        this.hashes.delete(copy);
      } catch (error) {
        const current = conflictCurrent(error);
        if (!current) throw error;
        // Saved again meanwhile: the copy shows the newest version.
        await this.refreshConflict(space, index, conflict, current);
      } finally {
        this.store.save(config.userId, space, index);
        this.emit();
      }
    });
  }

  /** Use theirs: their copy replaces yours, then anything edited in it pushes. */
  useTheirs(space: SpaceKey, docId: string, online: boolean): Promise<void> {
    return this.exclusive(async () => {
      const config = this.requireConfig();
      const state = await this.ensure(space, false);
      const index = state.index;
      const conflict = index?.conflicts.find((c) => c.docId === docId);
      if (!index || !conflict) return;
      const local = this.toLocal(space, conflict.path);
      const copy = this.toLocal(space, conflict.copyPath);
      if (!local || !copy) return;
      let theirs: string;
      try {
        theirs = await this.io.read(copy);
      } catch {
        // The copy was deleted by hand: take theirs from the server.
        const [doc] = await this.api.getDocuments(index.spaceId, [docId]);
        if (!doc || doc.deleted) return;
        theirs = doc.content;
      }
      await this.writeLocal(local, theirs);
      index.docs[docId] = { path: conflict.path, revision: conflict.revision, sha256: conflict.sha256 };
      index.conflicts = index.conflicts.filter((c) => c !== conflict);
      this.store.save(config.userId, space, index);
      await this.io.remove(copy).catch(() => {});
      this.hashes.delete(copy);
      await this.round(online);
    });
  }

  // Rounds -----------------------------------------------------------------

  private exclusive<T>(run: () => Promise<T>): Promise<T> {
    const next = this.chain.then(run, run);
    this.chain = next.catch(() => {});
    return next;
  }

  private requireConfig(): EngineConfig {
    if (!this.config) throw new Error("notes sync is off");
    return this.config;
  }

  private async round(online: boolean): Promise<void> {
    const config = this.config;
    if (!config) return;
    const files = await this.io.list();
    if (this.config !== config) return;
    const locals = await this.hashLocal(files, config.layout);

    let reached = online;
    let complete = online;
    for (const { key } of config.layout.spaces) {
      const local = locals.get(key) ?? {};
      if (reached) {
        try {
          await this.syncSpace(key, local);
          config.onSynced(key, Date.now());
        } catch (error) {
          if (error instanceof Abort || isOffline(error)) reached = false;
          // Anything else (a file that can't be read, say) waits for the next round.
          else console.warn("notes sync", key, error);
          complete = false;
        }
      }
      if (this.config !== config) return;
      this.measure(key, local);
    }
    this.offline = !reached;
    if (complete) this.syncedAt = Date.now();
    this.emit();
  }

  /** Hashes every file that syncs, by space, reading only what changed. */
  private async hashLocal(files: LocalFile[], layout: SpaceLayout): Promise<Map<SpaceKey, LocalHashes>> {
    const out = new Map<SpaceKey, LocalHashes>();
    for (const { key } of layout.spaces) out.set(key, {});
    const seen = new Set<string>();
    for (const file of files) {
      const where = spaceOfLocal(file.relPath, layout);
      if (!where) continue;
      seen.add(file.relPath);
      let cached = this.hashes.get(file.relPath);
      if (!cached || cached.mtimeMs !== file.mtimeMs || cached.size !== file.size) {
        try {
          const content = await this.io.read(file.relPath);
          cached = { mtimeMs: file.mtimeMs, size: file.size, sha: await sha256Hex(content) };
          this.hashes.set(file.relPath, cached);
        } catch {
          continue;
        }
      }
      out.get(where.space)![where.path] = cached.sha;
    }
    for (const path of [...this.hashes.keys()]) if (!seen.has(path)) this.hashes.delete(path);
    return out;
  }

  /** Loads the space's index, fetching its id from the server the first time. */
  private async ensure(space: SpaceKey, needServer: boolean): Promise<SpaceState> {
    const config = this.requireConfig();
    let state = this.spaces.get(space);
    if (!state) {
      state = { index: null, refused: new Map(), waiting: [], waitingCount: 0 };
      const stored = this.store.load(config.userId, space);
      if (stored) state.index = stored.root === config.root ? stored : emptyIndex(stored.spaceId, config.root);
      this.spaces.set(space, state);
    }
    if (!state.index && needServer) {
      const remote = await this.call(() =>
        space === "personal" ? this.api.getPersonalSpace() : this.api.getTeamSpace(space),
      );
      state.index = emptyIndex(remote.id, config.root);
      this.store.save(config.userId, space, state.index);
    }
    return state;
  }

  private async syncSpace(space: SpaceKey, local: LocalHashes): Promise<void> {
    const config = this.requireConfig();
    const state = await this.ensure(space, true);
    const index = state.index!;
    const save = () => this.store.save(config.userId, space, index);
    await this.pull(space, index, local);
    save();
    await this.push(space, state, local, save);
  }

  // Pull -------------------------------------------------------------------

  private async pull(space: SpaceKey, index: SpaceIndex, local: LocalHashes): Promise<void> {
    const config = this.requireConfig();
    let cursor = index.cursor;
    let changes: Change[] = [];
    for (let restarted = false; ; ) {
      let page: ChangePage;
      try {
        page = await this.call(() => this.api.listChanges(index.spaceId, cursor));
      } catch (error) {
        // The server's feed was restored to an earlier point: start over.
        if (!restarted && isCloudError(error, 410)) {
          restarted = true;
          cursor = "";
          changes = [];
          continue;
        }
        throw error;
      }
      changes.push(...page.changes);
      cursor = page.cursor;
      if (!page.hasMore) break;
    }

    // Notes held back while they were being edited come round again.
    const deferred = index.deferred.filter((id) => !changes.some((change) => change.docId === id));
    const again = (await this.fetch(index.spaceId, deferred)).map(asChange);
    index.deferred = [];

    const sim: LocalHashes = { ...local };
    const decisions: [Change, PullDecision][] = [];
    for (const change of latestChanges([...again, ...changes])) {
      const decision = decidePull(change, {
        entry: index.docs[change.docId],
        localSha: (path) => sim[path],
        editing: (path) => {
          const relPath = this.toLocal(space, path);
          return relPath !== null && config.editing(relPath);
        },
        inConflict: index.conflicts.some((c) => c.docId === change.docId),
        accepts: this.toLocal(space, change.path) !== null,
      });
      decisions.push([change, decision]);
      // Later changes see the folder as this one leaves it.
      if (decision.kind === "delete-local") delete sim[decision.path];
      if (decision.kind === "write" || decision.kind === "adopt") {
        sim[change.path] = change.sha256;
        if (decision.removeOld) delete sim[decision.removeOld];
      }
    }

    const needed = decisions
      .filter(([, d]) => d.kind === "write" || d.kind === "conflict" || d.kind === "refresh-conflict")
      .map(([change]) => change.docId);
    const docs = new Map((await this.fetch(index.spaceId, needed)).map((doc) => [doc.id, doc]));

    for (const [change, decision] of decisions) {
      const id = change.docId;
      const doc = docs.get(id);
      switch (decision.kind) {
        case "skip":
        case "keep-local":
          break;
        case "defer":
          index.deferred.push(id);
          break;
        case "forget":
          delete index.docs[id];
          break;
        case "delete-local":
          await this.removeLocal(space, decision.path, local);
          delete index.docs[id];
          break;
        case "adopt": {
          const sha = local[change.path];
          if (sha === undefined) break;
          index.docs[id] = { path: change.path, revision: change.revision, sha256: sha };
          if (decision.removeOld) await this.removeLocal(space, decision.removeOld, local);
          break;
        }
        case "write": {
          if (!doc || doc.deleted) break;
          const relPath = this.toLocal(space, doc.path);
          if (!relPath) break;
          const sha = await this.writeLocal(relPath, doc.content);
          local[doc.path] = sha;
          index.docs[id] = { path: doc.path, revision: doc.revision, sha256: sha };
          if (decision.removeOld && decision.removeOld !== doc.path) {
            await this.removeLocal(space, decision.removeOld, local);
          }
          break;
        }
        case "conflict":
          if (doc && !doc.deleted) await this.addConflict(space, index, id, doc.path, doc, local);
          break;
        case "refresh-conflict": {
          const conflict = index.conflicts.find((c) => c.docId === id);
          if (conflict && doc && !doc.deleted) await this.refreshConflict(space, index, conflict, doc);
          break;
        }
      }
    }
    index.cursor = cursor;
  }

  /** Documents by id, 100 at a time. */
  private async fetch(spaceId: string, ids: string[]): Promise<SyncDocument[]> {
    const out: SyncDocument[] = [];
    for (let i = 0; i < ids.length; i += PAGE_IDS) {
      const page = ids.slice(i, i + PAGE_IDS);
      out.push(...(await this.call(() => this.api.getDocuments(spaceId, page))));
    }
    return out;
  }

  // Push -------------------------------------------------------------------

  private skipped(state: SpaceState, local: LocalHashes): Set<string> {
    const skip = new Set<string>();
    for (const conflict of state.index?.conflicts ?? []) {
      skip.add(conflict.path);
      skip.add(conflict.copyPath);
    }
    for (const [path, sha] of state.refused) {
      if (local[path] === sha) skip.add(path);
      else state.refused.delete(path);
    }
    return skip;
  }

  private async push(space: SpaceKey, state: SpaceState, local: LocalHashes, save: () => void): Promise<void> {
    const index = state.index!;
    const ops = planPush(index, local, this.skipped(state, local), this.newId);
    for (const op of ops) {
      if (op.kind === "forget") {
        delete index.docs[op.docId];
        save();
        continue;
      }

      if (op.kind === "delete") {
        try {
          await this.call(() => this.api.deleteDocument(index.spaceId, op.docId, op.baseRevision));
          delete index.docs[op.docId];
        } catch (error) {
          const current = conflictCurrent(error);
          if (current && !current.deleted) {
            // Changed there after you last synced: their version comes back.
            const relPath = this.toLocal(space, current.path);
            if (relPath) {
              const sha = await this.writeLocal(relPath, current.content);
              local[current.path] = sha;
              index.docs[op.docId] = { path: current.path, revision: current.revision, sha256: sha };
            }
          } else if (current || isCloudError(error, 404)) {
            delete index.docs[op.docId];
          } else if (!isCloudError(error, 409)) {
            throw error;
          }
        }
        save();
        continue;
      }

      const relPath = this.toLocal(space, op.path);
      if (!relPath) continue;
      if (op.baseRevision === 0 && !index.docs[op.docId]) {
        // Kept before the call, so a retry sends the same id and can't create twice.
        index.docs[op.docId] = { path: op.path, revision: 0, sha256: "" };
        save();
      }
      let content: string;
      try {
        content = await this.io.read(relPath);
      } catch {
        continue;
      }
      const sha = await sha256Hex(content);
      try {
        const doc = await this.call(() =>
          this.api.putDocument(index.spaceId, op.docId, op.path, content, op.baseRevision),
        );
        index.docs[op.docId] = { path: doc.path, revision: doc.revision, sha256: sha };
      } catch (error) {
        const current = conflictCurrent(error);
        if (current) {
          await this.settlePut(space, index, op, sha, current, local);
        } else if (isCloudError(error, 409)) {
          // "The path was in use a moment ago": the next round tries again.
        } else if (isCloudError(error) && error.status >= 400 && error.status < 500) {
          // Too big or a path the server won't take: it stays on this computer.
          state.refused.set(op.path, sha);
          if (op.baseRevision === 0) delete index.docs[op.docId];
        } else {
          throw error;
        }
      }
      save();
    }
  }

  /** A put answered 409 with the stored document. */
  private async settlePut(
    space: SpaceKey,
    index: SpaceIndex,
    op: { docId: string; path: string; baseRevision: number },
    sha: string,
    current: SyncDocument,
    local: LocalHashes,
  ): Promise<void> {
    const theirs = current.deleted ? null : await sha256Hex(current.content);
    if (current.id !== op.docId) {
      // Another note already has this path on the server.
      if (op.baseRevision === 0) delete index.docs[op.docId];
      if (theirs === sha) index.docs[current.id] = { path: current.path, revision: current.revision, sha256: sha };
      else if (theirs !== null) await this.addConflict(space, index, current.id, op.path, current, local);
      return;
    }
    if (current.deleted) {
      // Deleted there while you edited it: yours brings it back next round.
      const entry = index.docs[op.docId];
      if (entry) entry.revision = current.revision;
      return;
    }
    if (theirs === sha) {
      // Same content (a retried create, or both sides made the same edit).
      index.docs[op.docId] = { path: current.path, revision: current.revision, sha256: sha };
      return;
    }
    await this.addConflict(space, index, op.docId, op.path, current, local);
  }

  // Conflicts --------------------------------------------------------------

  private async addConflict(
    space: SpaceKey,
    index: SpaceIndex,
    docId: string,
    path: string,
    current: SyncDocument,
    local: LocalHashes,
  ): Promise<void> {
    const config = this.requireConfig();
    const theirName = await config.nameOf(space, current.updatedBy);
    const taken = (candidate: string) =>
      local[candidate] !== undefined || index.conflicts.some((c) => c.copyPath === candidate);
    const copyPath = conflictCopyPath(path, theirName, taken);
    const relPath = this.toLocal(space, copyPath);
    if (!relPath) return;
    const sha = await this.writeLocal(relPath, current.content);
    local[copyPath] = sha;
    index.conflicts = index.conflicts.filter((c) => c.docId !== docId);
    index.conflicts.push({ docId, path, copyPath, revision: current.revision, sha256: sha, theirName });
  }

  private async refreshConflict(
    space: SpaceKey,
    index: SpaceIndex,
    conflict: SyncConflict,
    current: SyncDocument,
  ): Promise<void> {
    if (current.deleted) return;
    const config = this.requireConfig();
    const relPath = this.toLocal(space, conflict.copyPath);
    if (!relPath) return;
    const sha = await this.writeLocal(relPath, current.content);
    const theirName = await config.nameOf(space, current.updatedBy);
    index.conflicts = index.conflicts.map((c) =>
      c.docId === conflict.docId ? { ...c, revision: current.revision, sha256: sha, theirName } : c,
    );
  }

  // Local files ------------------------------------------------------------

  private toLocal(space: SpaceKey, path: string): string | null {
    return this.config ? localOfSpace(space, path, this.config.layout) : null;
  }

  private async writeLocal(relPath: string, content: string): Promise<string> {
    const sha = await sha256Hex(content);
    const { mtimeMs } = await this.io.write(relPath, content);
    this.hashes.set(relPath, { mtimeMs, size: byteLength(content), sha });
    return sha;
  }

  private async removeLocal(space: SpaceKey, path: string, local: LocalHashes): Promise<void> {
    const relPath = this.toLocal(space, path);
    delete local[path];
    if (!relPath) return;
    await this.io.remove(relPath).catch(() => {});
    this.hashes.delete(relPath);
  }

  // Status -----------------------------------------------------------------

  /** Works out what waits to push, without calling the server. */
  private measure(space: SpaceKey, local: LocalHashes): void {
    const state = this.spaces.get(space) ?? { index: null, refused: new Map(), waiting: [], waitingCount: 0 };
    this.spaces.set(space, state);
    if (!state.index && this.config) {
      const stored = this.store.load(this.config.userId, space);
      if (stored && stored.root === this.config.root) state.index = stored;
    }
    // Never synced: every note waits. The ids are placeholders, never sent.
    const ops = planPush(state.index ?? { docs: {} }, local, this.skipped(state, local), () => "");
    state.waiting = waitingPaths(ops)
      .map((path) => this.toLocal(space, path))
      .filter((path): path is string => path !== null);
    state.waitingCount = waitingCount(ops);
  }

  private emit(): void {
    const waiting: string[] = [];
    const conflicts: LocalConflict[] = [];
    const refused: string[] = [];
    let count = 0;
    for (const [space, state] of this.spaces) {
      waiting.push(...state.waiting);
      count += state.waitingCount;
      for (const path of state.refused.keys()) {
        const relPath = this.toLocal(space, path);
        if (relPath) refused.push(relPath);
      }
      for (const conflict of state.index?.conflicts ?? []) {
        const path = this.toLocal(space, conflict.path);
        const copyPath = this.toLocal(space, conflict.copyPath);
        if (path && copyPath) {
          conflicts.push({ space, docId: conflict.docId, path, copyPath, theirName: conflict.theirName });
        }
      }
    }
    this.snapshot = {
      waiting,
      waitingCount: count,
      conflicts,
      refused,
      offline: this.offline,
      syncedAt: this.syncedAt,
    };
    for (const listener of this.listeners) listener(this.snapshot);
  }

  /** Runs one API call; offline and signed out stop the round. */
  private async call<T>(run: () => Promise<T>): Promise<T> {
    try {
      return await run();
    } catch (error) {
      if (isOffline(error) || isCloudError(error, 401)) throw new Abort();
      throw error;
    }
  }
}

function isOffline(error: unknown): boolean {
  return isCloudError(error) && error.offline;
}

/** The stored document a 409 carries, if it carries one. */
function conflictCurrent(error: unknown): SyncDocument | null {
  if (!isCloudError(error, 409)) return null;
  const current = error.current as SyncDocument | undefined;
  return current && typeof current.id === "string" ? current : null;
}

function asChange(doc: SyncDocument): Change {
  return { docId: doc.id, path: doc.path, revision: doc.revision, deleted: doc.deleted, sha256: doc.sha256 };
}
