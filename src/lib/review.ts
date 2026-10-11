/**
 * Review comments and the message they become when they are sent to the
 * pane's agent (design turn 4). Pure helpers, unit tested.
 */

export interface CommentRef {
  path: string;
  line: number;
  text: string;
}

const HEADER = "Review comments from Augry";

function sortComments<T extends CommentRef>(comments: readonly T[]): T[] {
  return [...comments].sort((a, b) => a.path.localeCompare(b.path) || a.line - b.line);
}

/**
 * The text typed into the agent's prompt: one block per comment, grouped by
 * file in path order, then the optional note. Pasted as one bracketed paste,
 * so the newlines stay inside the prompt instead of submitting it.
 */
export function reviewMessage(comments: readonly CommentRef[], note: string): string {
  const lines = [`${HEADER} (${comments.length}). Please address them:`];
  for (const comment of sortComments(comments)) {
    lines.push("", `${comment.path}:${comment.line}`);
    for (const textLine of comment.text.trim().split(/\r?\n/)) lines.push(`  ${textLine}`);
  }
  const trimmed = note.trim();
  if (trimmed) lines.push("", trimmed);
  return lines.join("\n");
}

/**
 * The same message on one line, for a prompt that does not take bracketed
 * paste: every newline there would submit what came before it.
 */
export function reviewMessageOneLine(comments: readonly CommentRef[], note: string): string {
  const parts = sortComments(comments).map(
    (comment) =>
      `${comment.path}:${comment.line}: ${comment.text.trim().replace(/\s*\r?\n\s*/g, " ").replace(/[.;]+$/, "")}`,
  );
  const trimmed = note.trim().replace(/\s*\r?\n\s*/g, " ");
  const tail = trimmed ? ` Note: ${trimmed}` : "";
  return `${HEADER} (${comments.length}). Please address them: ${parts.join("; ")}.${tail}`;
}

/**
 * Where a comment sits now that the file may have moved under it: the line it
 * was written on when that still holds the same text, else the nearest line
 * with that text, else the original line number.
 */
export function reanchor(lines: readonly string[], line: number, lineText: string): number {
  if (lines[line - 1] === lineText) return line;
  const reach = 200;
  for (let distance = 1; distance <= reach; distance++) {
    if (lines[line - 1 - distance] === lineText) return line - distance;
    if (lines[line - 1 + distance] === lineText) return line + distance;
  }
  return Math.min(Math.max(1, line), Math.max(1, lines.length));
}

// File facts for the status bar ------------------------------------------------------

const LANGUAGES: Record<string, string> = {
  ts: "TypeScript", tsx: "TypeScript", mts: "TypeScript", cts: "TypeScript",
  js: "JavaScript", jsx: "JavaScript", mjs: "JavaScript", cjs: "JavaScript",
  rs: "Rust", py: "Python", go: "Go", rb: "Ruby", java: "Java", kt: "Kotlin", swift: "Swift",
  c: "C", h: "C", cpp: "C++", cc: "C++", hpp: "C++", cs: "C#", php: "PHP",
  css: "CSS", scss: "SCSS", html: "HTML", vue: "Vue", svelte: "Svelte",
  json: "JSON", md: "Markdown", yml: "YAML", yaml: "YAML", toml: "TOML", xml: "XML",
  sh: "Shell", bash: "Shell", zsh: "Shell", ps1: "PowerShell", sql: "SQL", lua: "Lua",
};

export function languageOf(path: string): string {
  const name = path.slice(path.lastIndexOf("/") + 1);
  const dot = name.lastIndexOf(".");
  if (dot <= 0) return "Plain text";
  return LANGUAGES[name.slice(dot + 1).toLowerCase()] ?? "Plain text";
}

export function lineEndingOf(text: string): "LF" | "CRLF" {
  const crlf = (text.match(/\r\n/g) ?? []).length;
  const lf = (text.match(/\n/g) ?? []).length - crlf;
  return crlf > lf ? "CRLF" : "LF";
}

/** "2 spaces", "4 spaces" or "Tabs", from how the lines are indented. */
export function indentOf(text: string): { label: string; unit: string } {
  let tabs = 0;
  const widths = new Map<number, number>();
  for (const line of text.split("\n")) {
    if (line.startsWith("\t")) tabs++;
    const spaces = /^( +)\S/.exec(line)?.[1]?.length;
    if (spaces) widths.set(spaces, (widths.get(spaces) ?? 0) + 1);
  }
  const spaced = [...widths.values()].reduce((sum, count) => sum + count, 0);
  if (tabs > spaced) return { label: "Tabs", unit: "\t" };
  if (spaced === 0) return { label: "2 spaces", unit: "  " };
  // Any indent that is not a multiple of four means the unit is two.
  const unit = [...widths.keys()].some((width) => width % 4 !== 0) ? 2 : 4;
  return { label: `${unit} spaces`, unit: " ".repeat(unit) };
}

/** "14:32" for the sent marker. */
export function clockTime(at: number): string {
  const date = new Date(at);
  return `${String(date.getHours()).padStart(2, "0")}:${String(date.getMinutes()).padStart(2, "0")}`;
}

/** "just now", "2m ago", "3h ago" for a comment thread. */
export function relativeTime(at: number, now: number): string {
  const seconds = Math.max(0, Math.floor((now - at) / 1000));
  if (seconds < 45) return "just now";
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.round(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  return `${Math.round(hours / 24)}d ago`;
}

/** Splits "src/ipc/types.ts" into the folder and the file name. */
export function splitPath(path: string): { dir: string; name: string } {
  const cut = path.lastIndexOf("/");
  return cut === -1 ? { dir: "", name: path } : { dir: path.slice(0, cut), name: path.slice(cut + 1) };
}
