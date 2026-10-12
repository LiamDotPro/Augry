/**
 * The decisions of notes sync, with no IO: what a pulled change does to the
 * local copy, and what local edits push. The engine (engine.ts) carries them
 * out. Paths here are a space's own paths (relative to its folder).
 *
 * One rule runs through all of it: a local file that changed since the last
 * sync is never overwritten. Pull leaves it alone, push sends it with the old
 * base revision, and the server's 409 turns it into a conflict the user picks.
 */
import type { Change } from "../cloud/types";

/** What this device last agreed with the server about one document. */
export interface IndexEntry {
  path: string;
  /** 0 while a create is in flight: the id is kept so a retry reuses it. */
  revision: number;
  /**
   * SHA-256 of the content as this device wrote or sent it (hashed here,
   * never taken from the server), so "changed since the last sync" is a
   * plain comparison with the file.
   */
  sha256: string;
}

/** A note both sides changed: yours stays at `path`, theirs is kept at `copyPath`. */
export interface SyncConflict {
  docId: string;
  path: string;
  copyPath: string;
  /** The server's revision, the base for Keep mine. */
  revision: number;
  /** SHA-256 of the server's content. */
  sha256: string;
  /** Who saved theirs: a teammate's name, or "another device". */
  theirName: string;
}

/** Everything one device keeps about one space. */
export interface SpaceIndex {
  spaceId: string;
  /** The notes folder this index describes; another folder starts over. */
  root: string;
  cursor: string;
  docs: Record<string, IndexEntry>;
  conflicts: SyncConflict[];
  /** Changes held back because the note was open with unsaved edits. */
  deferred: string[];
}

export function emptyIndex(spaceId: string, root: string): SpaceIndex {
  return { spaceId, root, cursor: "", docs: {}, conflicts: [], deferred: [] };
}

/** Local files of one space: path → SHA-256. */
export type LocalHashes = Record<string, string>;

// Pull -----------------------------------------------------------------------

export type PullDecision =
  /** Nothing to do here: already seen, or not a file this device syncs. */
  | { kind: "skip" }
  /** The file already has this content: only record the revision. */
  | { kind: "adopt"; removeOld?: string }
  /** Fetch it and write it at the change's path (and drop the old path of a rename). */
  | { kind: "write"; removeOld?: string }
  /** Deleted on the server and untouched here: delete the file. */
  | { kind: "delete-local"; path: string }
  /** Deleted on the server but changed here: keep the file, it pushes as a new note. */
  | { kind: "forget" }
  /** Open with unsaved edits: try again next round, after the save. */
  | { kind: "defer" }
  /** Changed here since the last sync: leave it; its push meets the 409. */
  | { kind: "keep-local" }
  /** A different local file already sits at that path: keep both. */
  | { kind: "conflict" }
  /** Already in conflict: refresh the kept copy with the newer version. */
  | { kind: "refresh-conflict" };

export interface PullContext {
  entry: IndexEntry | undefined;
  /** The local file's hash at a space path, undefined when there is none. */
  localSha(path: string): string | undefined;
  /** True while that note is open with edits not on disk yet. */
  editing(path: string): boolean;
  inConflict: boolean;
  /** False for paths this device doesn't hold (settings.json, a team folder's name in Personal). */
  accepts: boolean;
}

export function decidePull(change: Change, ctx: PullContext): PullDecision {
  const { entry } = ctx;
  if (ctx.inConflict) return change.deleted ? { kind: "skip" } : { kind: "refresh-conflict" };

  if (change.deleted) {
    if (!entry) return { kind: "skip" };
    const local = ctx.localSha(entry.path);
    if (local === undefined) return { kind: "forget" };
    if (local !== entry.sha256 || ctx.editing(entry.path)) return { kind: "forget" };
    return { kind: "delete-local", path: entry.path };
  }

  if (!ctx.accepts) return { kind: "skip" };
  // Our own write coming back, or a page seen before a resync.
  if (entry && entry.revision >= change.revision && entry.path === change.path) return { kind: "skip" };
  if (ctx.editing(change.path) || (entry && ctx.editing(entry.path))) return { kind: "defer" };

  const atTarget = ctx.localSha(change.path);
  const renamedFrom = entry && entry.path !== change.path ? entry.path : undefined;
  const oldUntouched = renamedFrom !== undefined && ctx.localSha(renamedFrom) === entry?.sha256;

  if (entry) {
    const atOld = ctx.localSha(entry.path);
    // Deleted here but changed there since: their newer version comes back.
    if (atOld === undefined) return atTarget === undefined ? { kind: "write" } : { kind: "conflict" };
    if (atOld !== entry.sha256) return { kind: "keep-local" };
    if (atTarget === change.sha256) return { kind: "adopt", removeOld: oldUntouched ? renamedFrom : undefined };
    if (renamedFrom !== undefined && atTarget !== undefined) return { kind: "conflict" };
    return { kind: "write", removeOld: renamedFrom };
  }

  if (atTarget === undefined) return { kind: "write" };
  if (atTarget === change.sha256) return { kind: "adopt" };
  return { kind: "conflict" };
}

/** One entry per document, the latest, in feed order. */
export function latestChanges(changes: Change[]): Change[] {
  const last = new Map<string, Change>();
  for (const change of changes) {
    last.delete(change.docId);
    last.set(change.docId, change);
  }
  return [...last.values()];
}

// Push -----------------------------------------------------------------------

export type PushOp =
  /** Create (baseRevision 0), update, or rename (`fromPath`) a document. */
  | { kind: "put"; docId: string; path: string; baseRevision: number; fromPath?: string }
  | { kind: "delete"; docId: string; path: string; baseRevision: number }
  /** A create that never reached the server and whose file is gone. */
  | { kind: "forget"; docId: string };

/**
 * What local edits push. `skip` holds paths that must not push yet (both
 * sides of a conflict, files the server refused); `newId` names new
 * documents (crypto.randomUUID in the app).
 */
export function planPush(
  index: Pick<SpaceIndex, "docs">,
  local: LocalHashes,
  skip: ReadonlySet<string>,
  newId: () => string,
): PushOp[] {
  const ops: PushOp[] = [];
  const indexed = new Set<string>();
  const missing: [string, IndexEntry][] = [];

  for (const [docId, entry] of Object.entries(index.docs)) {
    indexed.add(entry.path);
    if (skip.has(entry.path)) continue;
    const sha = local[entry.path];
    if (sha === undefined) missing.push([docId, entry]);
    else if (sha !== entry.sha256) ops.push({ kind: "put", docId, path: entry.path, baseRevision: entry.revision });
  }

  const fresh = Object.keys(local)
    .filter((path) => !indexed.has(path) && !skip.has(path))
    .sort();

  for (const [docId, entry] of missing) {
    if (entry.revision === 0) {
      ops.push({ kind: "forget", docId });
      continue;
    }
    // Same content at a new path: a rename (or a move), not a delete and a create.
    const to = fresh.find((path) => local[path] === entry.sha256);
    if (to !== undefined) {
      fresh.splice(fresh.indexOf(to), 1);
      ops.push({ kind: "put", docId, path: to, baseRevision: entry.revision, fromPath: entry.path });
    } else {
      ops.push({ kind: "delete", docId, path: entry.path, baseRevision: entry.revision });
    }
  }

  for (const path of fresh) ops.push({ kind: "put", docId: newId(), path, baseRevision: 0 });
  return ops;
}

/** Local paths with a change that hasn't reached the server ("waiting"). */
export function waitingPaths(ops: PushOp[]): string[] {
  return ops.flatMap((op) => (op.kind === "put" ? [op.path] : []));
}

/** How many changes wait to sync, as the offline footer counts them. */
export function waitingCount(ops: PushOp[]): number {
  return ops.filter((op) => op.kind !== "forget").length;
}
