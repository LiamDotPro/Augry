/**
 * Your processes on disk (src-tauri/src/processes): turning the files the
 * store found into records, and working out which files a change to the
 * records has to write. Pure, so it is tested on its own (disk.test.ts).
 */

import { formatProcess, hashOf, upgradeProcess, validateProcess } from "./json";
import type { Process, ProcessRecord, ProcessVersion } from "./types";
import { latest, withDraft } from "./versions";

/** One process as `processes_load` returns it. */
export interface StoredProcess {
  id: string;
  /** Saved versions, oldest first, as written. */
  versions: unknown[];
  draft: unknown | null;
  /** `<id>.json` as it is now. */
  file: unknown | null;
  problem: string | null;
}

export interface DiskProblem {
  id: string;
  message: string;
}

const isObj = (v: unknown): v is Record<string, unknown> => !!v && typeof v === "object" && !Array.isArray(v);
const usable = (v: unknown): v is Process => validateProcess(v).length === 0;

function asVersion(value: unknown): ProcessVersion | null {
  if (!isObj(value)) return null;
  value = { ...value, process: upgradeProcess(value.process) };
  if (!isObj(value) || typeof value.version !== "number" || !usable(value.process)) return null;
  return {
    version: value.version,
    message: typeof value.message === "string" ? value.message : "Saved",
    by: typeof value.by === "string" ? value.by : "You",
    at: typeof value.at === "number" ? value.at : 0,
    hash: typeof value.hash === "string" ? value.hash : hashOf(value.process),
    process: value.process,
    ...(typeof value.note === "string" ? { note: value.note } : {}),
  };
}

/**
 * Records from the files on disk. A process with no readable history but a
 * readable `<id>.json` (one written by hand) starts at version 1; a
 * `<id>.json` edited outside the app shows as a draft. Anything that can't be
 * read is left out and reported.
 */
export function recordsFromDisk(stored: StoredProcess[], now: number): { records: ProcessRecord[]; problems: DiskProblem[] } {
  const records: ProcessRecord[] = [];
  const problems: DiskProblem[] = [];
  for (const stored0 of stored) {
    // Files written before the shared format are read as it.
    const entry = { ...stored0, draft: upgradeProcess(stored0.draft), file: upgradeProcess(stored0.file) };
    const versions = entry.versions
      .map(asVersion)
      .filter((v): v is ProcessVersion => v !== null)
      .sort((a, b) => a.version - b.version);
    if (!versions.length && usable(entry.file)) {
      const process = { ...entry.file, version: 1 };
      versions.push({ version: 1, message: "Found in the processes folder", by: "You", at: now, hash: hashOf(process), process });
    }
    if (!versions.length) {
      problems.push({ id: entry.id, message: entry.problem ?? "It isn't a process file Augry can read" });
      continue;
    }
    if (entry.problem) problems.push({ id: entry.id, message: entry.problem });

    let record: ProcessRecord = { id: entry.id, versions, draft: null };
    if (usable(entry.draft)) record = withDraft(record, entry.draft);
    else if (usable(entry.file) && formatProcess({ ...entry.file, version: latest(record).version }) !== formatProcess(latest(record).process)) {
      record = withDraft(record, entry.file);
    }
    records.push(record);
  }
  return { records, problems };
}

/** What writing `after` over `before` takes, in order: saves, then drafts, then deletes. */
export interface DiskChanges {
  saves: { id: string; version: ProcessVersion }[];
  drafts: { id: string; draft: Process | null }[];
  deletes: string[];
}

export function diskChanges(before: ProcessRecord[], after: ProcessRecord[]): DiskChanges {
  const was = new Map(before.map((r) => [r.id, r]));
  const out: DiskChanges = { saves: [], drafts: [], deletes: [] };
  for (const record of after) {
    const old = was.get(record.id);
    if (old === record) continue;
    const saved = old ? latest(old).version : 0;
    const added = record.versions.filter((v) => v.version > saved);
    for (const version of added) out.saves.push({ id: record.id, version });
    // A save clears the draft file, so a draft kept after one is written again.
    if (record.draft !== (old?.draft ?? null) || (added.length > 0 && record.draft)) {
      out.drafts.push({ id: record.id, draft: record.draft });
    }
  }
  const now = new Set(after.map((r) => r.id));
  for (const record of before) if (!now.has(record.id)) out.deletes.push(record.id);
  return out;
}
