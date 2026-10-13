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

/**
 * Team spaces hold markdown only; personal holds .md too (settings.json is
 * out of scope). Case sensitive, as the server is: "Plan.MD" stays here.
 */
export function isSyncableNote(path: string): boolean {
  return path.endsWith(".md");
}

// The server's rules (apps/api/internal/docsync/path.go) ---------------------

/** A document's content may be at most 1 MiB of UTF-8. */
export const MAX_CONTENT_BYTES = 1 << 20;
/** A path may be at most 512 bytes of UTF-8. */
export const MAX_PATH_BYTES = 512;

export type SpaceKind = "team" | "personal";

export function kindOf(space: SpaceKey): SpaceKind {
  return space === PERSONAL ? "personal" : "team";
}

/** unicode.IsControl: C0 (tab and newline too), DEL and C1. */
const CONTROL = /[\u0000-\u001f\u007f-\u009f]/;
/** Half a surrogate pair: not valid UTF-8 once encoded. */
const LONE_SURROGATE = /[\ud800-\udbff](?![\udc00-\udfff])|(?<![\ud800-\udbff])[\udc00-\udfff]/;

/**
 * Why the server would refuse this path in a space of this kind, in its own
 * words, or null when it takes it. The rules, and the order they are checked
 * in, are path.go's ValidatePath.
 */
export function pathProblem(kind: SpaceKind, path: string): string | null {
  if (path === "") return "path is empty";
  if (new TextEncoder().encode(path).length > MAX_PATH_BYTES) return `path is longer than ${MAX_PATH_BYTES} bytes`;
  if (LONE_SURROGATE.test(path)) return "path is not valid UTF-8";
  if (CONTROL.test(path)) return "path has a control character";
  if (path.includes("\\")) return "path has a backslash";
  if (path.startsWith("/")) return "path starts with a slash";
  if (path.endsWith("/")) return "path ends with a slash";
  for (const segment of path.split("/")) {
    if (segment === "") return "path has an empty segment";
    if (segment === "." || segment === "..") return "path has a . or .. segment";
  }
  if (kind === "team") return path.endsWith(".md") ? null : "path must end in .md";
  return path.endsWith(".md") || path.endsWith(".json") ? null : "path must end in .md or .json";
}

/** What the "not synced" mark says about a note: why it stays on this computer. */
export function notSyncedReason(problem: string): string {
  return `Stays on this computer: ${problem}.`;
}

/** Why a note too large for the server stays here. */
export const TOO_LARGE = "content is larger than 1 MiB";

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
