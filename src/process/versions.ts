/**
 * Versions: what changed between two versions (the chips above a compare and
 * the draft card's summary), the folded line diff of their JSON, saving a
 * draft and restoring an old version. History only ever grows: restoring
 * saves the old content as a new version.
 */

import { diffLines } from "../lib/diff";
import { cloneProcess, formatProcess, hashOf } from "./json";
import { isRepeat, list, startTitle, surfaceMeta } from "./model";
import type { Process, ProcessRecord, ProcessVersion, Run, Step, VersionTag } from "./types";

export interface Change {
  sign: "+" | "-" | "~";
  text: string;
  /** The step it is about, to mark it in a lanes compare. */
  step?: string;
}

const FIELD_NAMES: Record<string, string> = {
  prompt: "prompt",
  run: "command",
  open: "page",
  saves: "output",
  approve: "what to approve",
  review: "what to review",
  ask: "question",
  question: "question",
  decide: "rule",
  doneWhen: "done when",
  onLimit: "after the last try",
};

const same = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b);

/** What changed from `a` to `b`, in the words the Compare chips use. */
export function summarizeChanges(a: Process, b: Process): Change[] {
  const out: Change[] = [];
  if (a.name !== b.name) out.push({ sign: "~", text: `Renamed process to ${b.name}` });
  if (!same(a.start, b.start)) out.push({ sign: "~", text: `Starts from: ${startTitle(b.start)}` });

  const before = new Map(a.steps.map((s) => [s.id, s]));
  const after = new Map(b.steps.map((s) => [s.id, s]));
  const added = b.steps.filter((s) => !before.has(s.id));
  const removed = a.steps.filter((s) => !after.has(s.id));
  const touched = new Set([...added, ...removed].map((s) => s.id));

  for (const s of added) {
    const meta = surfaceMeta(b.surfaces.find((x) => x.id === s.on));
    out.push({ sign: "+", text: `${s.name} on ${meta.name}`, step: s.id });
  }
  for (const s of removed) out.push({ sign: "-", text: s.name, step: s.id });

  for (const s of b.steps) {
    const old = before.get(s.id);
    if (!old) continue;
    const parts: string[] = [];
    let renamed = "";
    for (const key of new Set([...Object.keys(old), ...Object.keys(s)])) {
      if (key === "id" || same(old[key], s[key])) continue;
      if (key === "name") renamed = `Renamed ${old.name} to ${s.name}`;
      else if (key === "on") parts.push(`moved to ${surfaceMeta(b.surfaces.find((x) => x.id === s.on)).name}`);
      else if (key === "no") parts.push(noChange(old, s));
      else if (key === "after" || key === "yes") {
        // Wiring that only changed because a step was added or removed is not news.
        const ids = (x: Step) => new Set([...list(x.after), ...list(x.yes)].filter((t) => !touched.has(t)));
        if (!same([...ids(old)].sort(), [...ids(s)].sort()) && !parts.includes("order")) parts.push("order");
      } else if (key === "stage") parts.push("stage");
      else parts.push(FIELD_NAMES[key] ?? key);
    }
    if (renamed) out.push({ sign: "~", text: renamed, step: s.id });
    const unique = [...new Set(parts)];
    if (unique.length) out.push({ sign: "~", text: `${s.name}: ${unique.join(", ")}`, step: s.id });
  }

  const stagesBefore = new Map(a.stages.map((g) => [g.id, g.name]));
  for (const g of b.stages) {
    const was = stagesBefore.get(g.id);
    if (was === undefined) out.push({ sign: "+", text: `Stage ${g.name}` });
    else if (was !== g.name) out.push({ sign: "~", text: `Renamed stage ${was} to ${g.name}` });
  }
  for (const g of a.stages) if (!b.stages.some((x) => x.id === g.id)) out.push({ sign: "-", text: `Stage ${g.name}` });

  const rowsBefore = new Map(a.surfaces.map((s) => [s.id, s]));
  for (const s of b.surfaces) {
    const was = rowsBefore.get(s.id);
    if (!was) out.push({ sign: "+", text: `Row ${surfaceMeta(s).name}` });
    else if (!same(was, s)) out.push({ sign: "~", text: `${surfaceMeta(s).name} row settings` });
  }
  if (!same(a.loops ?? [], b.loops ?? [])) out.push({ sign: "~", text: "Loops" });
  if (!same(a.ends ?? [], b.ends ?? [])) out.push({ sign: "~", text: "Ends" });
  return out;
}

function noChange(old: Step, now: Step): string {
  if (isRepeat(old.no) && isRepeat(now.no)) {
    if (old.no.max !== now.no.max && old.no.repeatFrom === now.no.repeatFrom) return `max ${old.no.max} → ${now.no.max}`;
    return "loop";
  }
  return "no branch";
}

/** The draft card's one line: "Renamed Smoke test, Smoke test: page". */
export function changeSentence(changes: Change[]): string {
  if (!changes.length) return "No changes";
  const text = changes.map((c) => (c.sign === "+" ? `Added ${c.text}` : c.sign === "-" ? `Removed ${c.text}` : c.text));
  return text.slice(0, 3).join(", ") + (text.length > 3 ? ` and ${text.length - 3} more` : "");
}

// Line diff -----------------------------------------------------------------------

export type DiffLine =
  | { kind: "ctx" | "add" | "del"; text: string; old: number | null; new: number | null }
  | { kind: "fold"; count: number };

/** The JSON of `a` against `b`, with unchanged runs folded down to two lines of context. */
export function versionDiff(a: Process, b: Process, context = 2): DiffLine[] {
  const rows = diffLines(formatProcess(a).split("\n"), formatProcess(b).split("\n"));
  const keep = rows.map((_, k) => rows.slice(Math.max(0, k - context), k + context + 1).some((r) => r.kind !== "ctx"));
  const out: DiffLine[] = [];
  let skipped = 0;
  rows.forEach((row, k) => {
    if (!keep[k]) {
      skipped++;
      return;
    }
    if (skipped) out.push({ kind: "fold", count: skipped });
    skipped = 0;
    out.push(row);
  });
  if (skipped) out.push({ kind: "fold", count: skipped });
  return out;
}

// History ---------------------------------------------------------------------------

export function latest(record: ProcessRecord): ProcessVersion {
  return record.versions[record.versions.length - 1]!;
}

/** What the editor shows: the draft if there is one, else the latest version. */
export function working(record: ProcessRecord): Process {
  return record.draft ?? latest(record).process;
}

export function versionOf(record: ProcessRecord, version: number): ProcessVersion | undefined {
  return record.versions.find((v) => v.version === version);
}

/** Saves the draft (or `content`) as the next version. */
export function saveVersion(
  record: ProcessRecord,
  message: string,
  now: number,
  options: { content?: Process; note?: string; by?: string } = {},
): ProcessRecord {
  const source = options.content ?? record.draft;
  if (!source) return record;
  const version = latest(record).version + 1;
  const process = { ...cloneProcess(source), version };
  const entry: ProcessVersion = {
    version,
    message: message.trim() || "Saved",
    by: options.by ?? "You",
    at: now,
    hash: hashOf(process),
    process,
    note: options.note,
  };
  return { ...record, versions: [...record.versions, entry], draft: null };
}

/** Restoring saves the old content as a new version; any draft is dropped. */
export function restoreVersion(record: ProcessRecord, version: number, now: number): ProcessRecord {
  const old = versionOf(record, version);
  if (!old) return record;
  return saveVersion({ ...record, draft: null }, `Restored v${version}`, now, { content: old.process, note: `restored v${version}` });
}

/** Sets the draft, or clears it when the edit lands back on the saved version. */
export function withDraft(record: ProcessRecord, draft: Process): ProcessRecord {
  const saved = latest(record).process;
  const normalized = { ...draft, version: saved.version };
  return { ...record, draft: formatProcess(normalized) === formatProcess(saved) ? null : normalized };
}

/** Tags beside a version in the list: current, the runs that used it, a restore. */
export function versionTags(record: ProcessRecord, version: ProcessVersion, runs: Run[], compared: number | null): VersionTag[] {
  const tags: VersionTag[] = [];
  if (version.version === latest(record).version) tags.push({ text: "current", tone: "current" });
  const used = runs
    .filter((r) => r.processId === record.id && r.version === version.version)
    .map((r) => r.number)
    .sort((x, y) => x - y);
  if (used.length === 1) tags.push({ text: `run ${used[0]}`, tone: "run" });
  else if (used.length > 1) {
    const contiguous = used.every((n, i) => i === 0 || n === used[i - 1]! + 1);
    tags.push({ text: contiguous ? `runs ${used[0]}–${used[used.length - 1]}` : `${used.length} runs`, tone: "run" });
  }
  if (compared === version.version) tags.push({ text: "compared", tone: "compared" });
  if (version.note) tags.push({ text: version.note, tone: "muted" });
  return tags;
}
