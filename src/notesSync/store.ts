/**
 * Where each space's index (plan.ts SpaceIndex) is kept: localStorage, keyed
 * by user id and space. It is per device by nature (it describes this
 * computer's files), small (paths, revisions and hashes, never content), and
 * only the renderer reads it, so it doesn't need a Rust command or the
 * session file. Keying by user means another account on the same computer
 * starts from nothing instead of pushing into the wrong space; the index also
 * remembers the notes folder it describes, and a different folder starts over.
 */
import { teamFolderName } from "./paths";
import type { SpaceIndex } from "./plan";

const PREFIX = "augry.notesSync.v1";

export function indexKey(userId: string, space: string): string {
  return `${PREFIX}.${userId}.${space}`;
}

function storage(): Storage | null {
  try {
    return typeof localStorage === "undefined" ? null : localStorage;
  } catch {
    return null;
  }
}

function isIndex(value: unknown): value is SpaceIndex {
  const index = value as SpaceIndex | null;
  return (
    !!index &&
    typeof index.spaceId === "string" &&
    typeof index.root === "string" &&
    typeof index.cursor === "string" &&
    typeof index.docs === "object" &&
    Array.isArray(index.conflicts) &&
    Array.isArray(index.deferred)
  );
}

export function loadIndex(userId: string, space: string): SpaceIndex | null {
  try {
    const raw = storage()?.getItem(indexKey(userId, space));
    if (!raw) return null;
    const parsed: unknown = JSON.parse(raw);
    return isIndex(parsed) ? parsed : null;
  } catch {
    return null;
  }
}

export function saveIndex(userId: string, space: string, index: SpaceIndex): void {
  try {
    storage()?.setItem(indexKey(userId, space), JSON.stringify(index));
  } catch {
    // Full or blocked: the next round rebuilds what it needs from the server.
  }
}

/**
 * The server's features as last seen by this user here. Started offline,
 * the app hasn't heard them yet, and sync should still look the way it did
 * (offline, with changes waiting) rather than vanish.
 */
export function lastFeatures(userId: string): string[] {
  try {
    const parsed: unknown = JSON.parse(storage()?.getItem(`${PREFIX}.${userId}.features`) ?? "[]");
    return Array.isArray(parsed) ? parsed.filter((item): item is string => typeof item === "string") : [];
  } catch {
    return [];
  }
}

export function rememberFeatures(userId: string, features: string[]): void {
  try {
    storage()?.setItem(`${PREFIX}.${userId}.features`, JSON.stringify(features));
  } catch {
    // Blocked: offline starts then show sync once the server answers.
  }
}

/** Member counts as last seen, so "team · 6" still reads right offline. */
export function lastMemberCounts(userId: string): Record<string, number> {
  try {
    const parsed: unknown = JSON.parse(storage()?.getItem(`${PREFIX}.${userId}.members`) ?? "{}");
    return parsed && typeof parsed === "object" ? (parsed as Record<string, number>) : {};
  } catch {
    return {};
  }
}

export function rememberMemberCount(userId: string, teamId: string, count: number): void {
  try {
    const counts = { ...lastMemberCounts(userId), [teamId]: count };
    storage()?.setItem(`${PREFIX}.${userId}.members`, JSON.stringify(counts));
  } catch {
    // Blocked: the count shows once the team loads.
  }
}

const FOLDERS = `${PREFIX}.teamFolders`;

/**
 * The folder a team's notes live in on this computer. It is named after the
 * team the first time sync uses it and then kept, so renaming the team
 * doesn't leave its notes in a folder that would read as personal.
 */
export function teamFolderFor(teamId: string, teamName: string, remember: boolean): string {
  let folders: Record<string, string> = {};
  try {
    folders = JSON.parse(storage()?.getItem(FOLDERS) ?? "{}") as Record<string, string>;
  } catch {
    folders = {};
  }
  const known = folders[teamId];
  if (typeof known === "string" && known) return known;
  const folder = teamFolderName(teamName);
  if (remember) {
    try {
      storage()?.setItem(FOLDERS, JSON.stringify({ ...folders, [teamId]: folder }));
    } catch {
      // Blocked: the name is worked out again next time.
    }
  }
  return folder;
}
