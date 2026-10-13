/**
 * The process file as text: formatting (short objects stay on one line, like a
 * formatter would), tokens for the highlighted JSON panel, parsing with the
 * line of the first error, a structural check of the format (schemaVersion 2), and the
 * content hash that versions and runs record.
 */

import { isRepeat, list } from "./model";
import { MAX_NAME, PROCESS_FORMAT, type Process } from "./types";

const WIDTH = 96;

/** Keys whose value is undefined are left out, as JSON.stringify does. */
const entriesOf = (v: object) => Object.entries(v).filter(([, x]) => x !== undefined);

/** A value on one line. */
function inline(v: unknown): string {
  if (v === null || typeof v !== "object") return JSON.stringify(v) ?? "null";
  if (Array.isArray(v)) return "[" + v.map(inline).join(", ") + "]";
  if (!entriesOf(v).length) return "{}";
  return "{ " + entriesOf(v).map(([k, x]) => JSON.stringify(k) + ": " + inline(x)).join(", ") + " }";
}

function fmt(v: unknown, indent: number, before: number): string {
  const one = inline(v);
  if (v === null || typeof v !== "object" || indent + before + one.length <= WIDTH) return one;
  const pad = " ".repeat(indent);
  const pad2 = " ".repeat(indent + 2);
  if (Array.isArray(v)) {
    if (!v.length) return "[]";
    return "[\n" + v.map((x) => pad2 + fmt(x, indent + 2, 0)).join(",\n") + "\n" + pad + "]";
  }
  const entries = entriesOf(v);
  if (!entries.length) return "{}";
  return (
    "{\n" +
    entries.map(([k, x]) => pad2 + JSON.stringify(k) + ": " + fmt(x, indent + 2, k.length + 4)).join(",\n") +
    "\n" +
    pad +
    "}"
  );
}

/** The file's text. Stable: formatting a parsed format gives the same text. */
export function formatProcess(value: unknown): string {
  return fmt(value, 0, 0);
}

// Tokens -------------------------------------------------------------------------

export type TokenKind = "key" | "string" | "number" | "literal" | "punct" | "plain";

export interface Token {
  text: string;
  kind: TokenKind;
}

const TOK = /("(?:[^"\\]|\\.)*")(\s*:)?|(-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?)|\b(true|false|null)\b|([{}[\],])/g;

export function tokenize(line: string): Token[] {
  const out: Token[] = [];
  let last = 0;
  TOK.lastIndex = 0;
  let m: RegExpExecArray | null;
  while ((m = TOK.exec(line))) {
    if (m.index > last) out.push({ text: line.slice(last, m.index), kind: "plain" });
    if (m[1]) {
      out.push({ text: m[1], kind: m[2] ? "key" : "string" });
      if (m[2]) out.push({ text: m[2], kind: "punct" });
    } else if (m[3]) out.push({ text: m[3], kind: "number" });
    else if (m[4]) out.push({ text: m[4], kind: "literal" });
    else out.push({ text: m[5]!, kind: "punct" });
    last = TOK.lastIndex;
  }
  if (last < line.length) out.push({ text: line.slice(last), kind: "plain" });
  return out;
}

// Selection ------------------------------------------------------------------------

/**
 * The lines (0-based, inclusive) of the step object with this id in formatted
 * text, or null. A step that fits on one line is that line; otherwise it runs
 * from its `{` to the matching `}`.
 */
export function stepLineRange(lines: readonly string[], id: string): [number, number] | null {
  const needle = `"id": ${JSON.stringify(id)}`;
  // Rows and stages come before the steps and can share a step's id, so only
  // look inside the steps list.
  const steps = lines.findIndex((l) => /^\s*"steps": \[/.test(l));
  if (steps < 0) return null;
  const found = lines.slice(steps + 1).findIndex((l) => l.includes(needle));
  if (found < 0) return null;
  const i = steps + 1 + found;
  const line = lines[i]!;
  if (line.trimStart().startsWith("{")) return [i, i];
  const open = i - 1;
  if (open < 0) return [i, i];
  const indent = lines[open]!.match(/^\s*/)![0].length;
  for (let k = i + 1; k < lines.length; k++) {
    const l = lines[k]!;
    if (l.match(/^\s*/)![0].length === indent && /^\s*\}/.test(l)) return [open, k];
  }
  return [open, i];
}

/** The id of the step whose lines contain `line`, if any. */
export function stepAtLine(lines: readonly string[], line: number, ids: readonly string[]): string | null {
  for (const id of ids) {
    const r = stepLineRange(lines, id);
    if (r && line >= r[0] && line <= r[1]) return id;
  }
  return null;
}

// Parsing ----------------------------------------------------------------------------

export type ParseResult = { ok: true; process: Process } | { ok: false; error: string; line: number | null };

/** The 1-based line of a JSON.parse error, from either message style V8 uses. */
export function errorLine(message: string, text: string): number | null {
  const lc = message.match(/line (\d+) column \d+/);
  if (lc) return Number(lc[1]);
  const pos = message.match(/position (\d+)/);
  if (pos) return text.slice(0, Number(pos[1])).split("\n").length;
  return null;
}

/** Parses the text and checks it is a usable process. */
export function parseProcess(text: string): ParseResult {
  let value: unknown;
  try {
    value = JSON.parse(text);
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    // The line is shown separately, so the position V8 appends is dropped.
    const short = message.replace(/^JSON\.parse: /, "").replace(/\s*in JSON at position \d+(\s*\(line \d+ column \d+\))?/, "");
    return { ok: false, error: short, line: errorLine(message, text) };
  }
  value = upgradeProcess(value);
  const problems = validateProcess(value);
  if (problems.length) {
    const first = problems[0]!;
    const lines = text.split("\n");
    const at = first.near ? lines.findIndex((l) => l.includes(first.near!)) : -1;
    return { ok: false, error: first.message, line: at >= 0 ? at + 1 : null };
  }
  return { ok: true, process: value as Process };
}

export interface Problem {
  message: string;
  /** Text on the line the problem is about, to find it in the file. */
  near?: string;
}

const isObj = (v: unknown): v is Record<string, unknown> => !!v && typeof v === "object" && !Array.isArray(v);
const quoted = (id: string) => `"id": ${JSON.stringify(id)}`;

/** The old marker, `"$schema": "augry/process@1"`, of files written before schemaVersion 2. */
const OLD_MARKER = "augry/process@1";

/**
 * A process in the shared format. Files from before schemaVersion 2 carry the
 * same graph under `"$schema": "augry/process@1"`; they are upgraded as they
 * are read. Anything else is returned as it was.
 */
export function upgradeProcess(value: unknown): unknown {
  if (!isObj(value) || value.schemaVersion !== undefined || value.$schema !== OLD_MARKER) return value;
  const { $schema: _old, ...rest } = value;
  return { schemaVersion: PROCESS_FORMAT, ...rest };
}

/** A name the server takes: 1 to 120 characters, counted as it counts them. */
export function nameProblem(name: unknown): string | null {
  if (typeof name !== "string") return "name must be a string";
  const length = [...name].length;
  if (length === 0) return "name can't be empty";
  if (length > MAX_NAME) return `name is ${length} characters; ${MAX_NAME} at most`;
  return null;
}

/** Cuts a name to the longest the server takes, by characters. */
export function fitName(name: string): string {
  const chars = [...name];
  return chars.length > MAX_NAME ? chars.slice(0, MAX_NAME).join("").trimEnd() : name;
}

/** Structural checks of the format (schemaVersion 2). Empty when the process is usable. */
export function validateProcess(value: unknown): Problem[] {
  const out: Problem[] = [];
  if (!isObj(value)) return [{ message: "A process is a JSON object" }];
  if (value.schemaVersion !== PROCESS_FORMAT) out.push({ message: `schemaVersion must be ${PROCESS_FORMAT}`, near: '"schemaVersion"' });
  if (typeof value.id !== "string" || !value.id) out.push({ message: "id must be a string", near: '"id"' });
  const badName = nameProblem(value.name);
  if (badName) out.push({ message: badName, near: '"name"' });
  if (typeof value.version !== "number") out.push({ message: "version must be a number", near: '"version"' });
  if (!isObj(value.start) || typeof value.start.on !== "string") out.push({ message: "start needs an `on`", near: '"start"' });
  for (const key of ["surfaces", "stages", "steps"] as const) {
    if (!Array.isArray(value[key])) out.push({ message: `${key} must be a list`, near: `"${key}"` });
  }
  if (out.length) return out;

  const p = value as unknown as Process;
  const surfaces = new Set<string>();
  for (const s of p.surfaces) {
    if (!isObj(s) || typeof s.id !== "string") return [{ message: "Every row needs an id", near: '"surfaces"' }];
    surfaces.add(s.id);
  }
  const stages = new Set<string>();
  for (const g of p.stages) {
    if (!isObj(g) || typeof g.id !== "string") return [{ message: "Every stage needs an id", near: '"stages"' }];
    stages.add(g.id);
  }
  const ids = new Set<string>();
  for (const s of p.steps) {
    if (!isObj(s) || typeof s.id !== "string") return [{ message: "Every step needs an id", near: '"steps"' }];
    if (ids.has(s.id)) out.push({ message: `Two steps are called "${s.id}"`, near: quoted(s.id) });
    ids.add(s.id);
    if (typeof s.name !== "string") out.push({ message: `Step "${s.id}" needs a name`, near: quoted(s.id) });
    if (!surfaces.has(s.on)) out.push({ message: `Step "${s.id}" is on "${String(s.on)}", which is not a row`, near: quoted(s.id) });
    if (!stages.has(s.stage)) out.push({ message: `Step "${s.id}" is in "${String(s.stage)}", which is not a stage`, near: quoted(s.id) });
  }
  for (const s of p.steps) {
    const refs = [...list(s.after), ...(s.decide ? list(s.yes) : []), ...(Array.isArray(s.no) ? s.no : [])];
    if (isRepeat(s.no)) refs.push(s.no.repeatFrom);
    for (const r of refs) {
      if (!ids.has(r)) out.push({ message: `Step "${s.id}" points at "${r}", which is not a step`, near: quoted(s.id) });
    }
  }
  for (const key of ["ends", "loops"] as const) {
    const items = value[key];
    if (items === undefined) continue;
    if (!Array.isArray(items) || !items.every(isObj)) return [...out, { message: `${key} must be a list of objects`, near: `"${key}"` }];
  }
  for (const l of p.loops ?? []) {
    if (l.steps !== undefined && !Array.isArray(l.steps)) {
      return [...out, { message: `Loop "${String(l.id)}" needs steps as a list`, near: '"loops"' }];
    }
  }
  for (const e of p.ends ?? []) {
    if (!ids.has(e.after)) out.push({ message: `End "${e.name}" comes after "${e.after}", which is not a step`, near: '"ends"' });
  }
  for (const l of p.loops ?? []) {
    for (const id of l.steps ?? []) {
      if (!ids.has(id)) out.push({ message: `Loop "${l.id}" repeats "${id}", which is not a step`, near: '"loops"' });
    }
  }
  if (!out.length && hasCycle(p)) out.push({ message: "Steps wait on each other in a circle", near: '"steps"' });
  return out;
}

function hasCycle(p: Process): boolean {
  const next: Record<string, string[]> = {};
  for (const s of p.steps) next[s.id] = [];
  for (const s of p.steps) {
    for (const a of list(s.after)) next[a]?.push(s.id);
    if (s.decide) {
      for (const t of list(s.yes)) next[s.id]!.push(t);
      if (Array.isArray(s.no)) for (const t of s.no) next[s.id]!.push(t);
    }
  }
  const state: Record<string, 0 | 1 | 2> = {};
  const visit = (id: string): boolean => {
    if (state[id] === 1) return true;
    if (state[id] === 2) return false;
    state[id] = 1;
    for (const n of next[id] ?? []) if (visit(n)) return true;
    state[id] = 2;
    return false;
  };
  return p.steps.some((s) => visit(s.id));
}

/** A short content hash (FNV-1a), shown on versions and pinned by runs. */
export function hashOf(process: Process): string {
  const text = formatProcess(process);
  let h = 0x811c9dc5;
  for (let i = 0; i < text.length; i++) {
    h ^= text.charCodeAt(i);
    h = Math.imul(h, 0x01000193);
  }
  return (h >>> 0).toString(16).padStart(8, "0").slice(0, 7);
}

/** A deep copy through JSON, which is all a process ever is. */
export function cloneProcess<T>(value: T): T {
  return JSON.parse(JSON.stringify(value)) as T;
}
