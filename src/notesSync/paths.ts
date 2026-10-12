/**
 * Where each synced space lives in the notes folder (Notes board 02). The
 * personal space is the notes folder itself, minus the team folders; each
 * team's space is the folder `<notes folder>/<Team name>`. Paths here are
 * relative to the notes folder with forward slashes, as notes_tree gives them;
 * a space's own paths are relative to its folder, as the API wants them.
 */

/** "personal" or a team id. */
export type SpaceKey = string;

export const PERSONAL: SpaceKey = "personal";

export interface SpaceLayout {
  /** The spaces that sync, with the folder each one owns ("" for personal). */
  spaces: { key: SpaceKey; folder: string }[];
  /** Every team's folder, synced or not: none of them belong to Personal. */
  teamFolders: string[];
}

/** Characters no file system takes in a folder name. */
const UNSAFE = /[\\/:*?"<>|\u0000-\u001f]/g;

/** A name made safe to use as one file or folder name. */
export function safeName(name: string, fallback: string): string {
  const cleaned = name
    .replace(UNSAFE, "-")
    .trim()
    .replace(/^\.+/, "")
    .replace(/[. ]+$/, "");
  return cleaned || fallback;
}

/** The folder a team's shared notes live in, under the notes folder. */
export function teamFolderName(teamName: string): string {
  return safeName(teamName, "Team");
}

/** Team spaces hold markdown only; personal holds .md too (settings.json is out of scope). */
export function isSyncableNote(path: string): boolean {
  return path.toLowerCase().endsWith(".md");
}

function firstSegment(relPath: string): string {
  const slash = relPath.indexOf("/");
  return slash === -1 ? relPath : relPath.slice(0, slash);
}

/**
 * The synced space a local file belongs to and its path in that space, or
 * null when it doesn't sync (not markdown, a team folder that isn't synced,
 * or Personal with personal sync off).
 */
export function spaceOfLocal(relPath: string, layout: SpaceLayout): { space: SpaceKey; path: string } | null {
  if (!isSyncableNote(relPath)) return null;
  const head = firstSegment(relPath);
  const inside = relPath.includes("/");
  for (const { key, folder } of layout.spaces) {
    if (folder && inside && head === folder) return { space: key, path: relPath.slice(folder.length + 1) };
  }
  if (inside && layout.teamFolders.includes(head)) return null;
  // A top-level file named like a team folder is still personal.
  const personal = layout.spaces.find((space) => space.folder === "");
  return personal ? { space: personal.key, path: relPath } : null;
}

/**
 * Where a space's document lands in the notes folder, or null when it can't
 * land here: a personal path that would fall inside a team folder, or a file
 * this app doesn't sync.
 */
export function localOfSpace(space: SpaceKey, path: string, layout: SpaceLayout): string | null {
  if (!isSyncableNote(path)) return null;
  const entry = layout.spaces.find((candidate) => candidate.key === space);
  if (!entry) return null;
  if (entry.folder) return `${entry.folder}/${path}`;
  if (path.includes("/") && layout.teamFolders.includes(firstSegment(path))) return null;
  return path;
}

/**
 * The name the other version of a conflicted note is kept under, next to
 * yours: `claude-code (Maya Chen).md`, then `claude-code (Maya Chen 2).md`
 * if that is taken.
 */
export function conflictCopyPath(path: string, theirName: string, taken: (path: string) => boolean): string {
  const slash = path.lastIndexOf("/");
  const dir = slash === -1 ? "" : path.slice(0, slash + 1);
  const file = path.slice(slash + 1);
  const dot = file.toLowerCase().endsWith(".md") ? file.length - 3 : file.length;
  const stem = file.slice(0, dot);
  const who = safeName(theirName, "another device");
  for (let n = 1; ; n++) {
    const candidate = `${dir}${stem} (${who}${n === 1 ? "" : ` ${n}`}).md`;
    if (!taken(candidate)) return candidate;
  }
}

/** The separator the notes folder path uses, so a team folder reads like the rest. */
export function separatorOf(root: string): "\\" | "/" {
  return root.includes("\\") && !root.includes("/") ? "\\" : "/";
}

/** `<notes folder><sep><Team name>`, for the setup dialog. */
export function joinRoot(root: string, folder: string): string {
  const sep = separatorOf(root);
  return root.endsWith(sep) ? `${root}${folder}` : `${root}${sep}${folder}`;
}
