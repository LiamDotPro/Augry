/**
 * Team processes (Process board 11) as the tab holds them. The server is the
 * source of truth: each team has a small cache here, its processes by id and
 * how far the change feed got (`lastSeq`), filled from
 * `GET /v1/teams/{id}/processes?since=`. The cards, editor and versions draw
 * ProcessRecords, so each live process becomes one, keyed by its server id.
 */

import type { Member, ProcessPage, ProcessVersionInfo, TeamProcess } from "../cloud/types";
import { hashOf, validateProcess } from "./json";
import { whenLabel } from "./runs";
import { isV1, processFromV1 } from "./v1";
import type { Process, ProcessRecord, ProcessVersion } from "./types";
import { latest } from "./versions";

export interface TeamCache {
  /** Live processes by id; tombstones are dropped. */
  processes: Record<string, TeamProcess>;
  /** Where the next `since` starts. */
  lastSeq: number;
}

export const EMPTY_TEAM_CACHE: TeamCache = { processes: {}, lastSeq: 0 };

/**
 * One page of the change feed applied to the cache. While more pages follow,
 * the cursor only moves to the last item read, so an interrupted load picks
 * up where it stopped instead of skipping what it never saw.
 */
export function applyPage(cache: TeamCache, page: ProcessPage): TeamCache {
  const processes = { ...cache.processes };
  for (const p of page.processes) {
    if (p.deleted) delete processes[p.id];
    else processes[p.id] = p;
  }
  const last = page.processes[page.processes.length - 1];
  const lastSeq = page.hasMore && last ? last.seq : Math.max(cache.lastSeq, page.lastSeq);
  return { processes, lastSeq };
}

/** Puts a process the client just wrote (save, restore, create) into the cache. */
export function putProcess(cache: TeamCache, process: TeamProcess): TeamCache {
  const known = cache.processes[process.id];
  if (known && known.version > process.version) return cache;
  return { ...cache, processes: { ...cache.processes, [process.id]: process } };
}

export function dropProcess(cache: TeamCache, id: string): TeamCache {
  if (!cache.processes[id]) return cache;
  const { [id]: _gone, ...processes } = cache.processes;
  return { ...cache, processes };
}

/**
 * Reads the feed from `since` until it says there's no more, handing each page
 * on as it arrives. The next `since` is the last item's seq.
 */
export async function readFeed(list: (since: number) => Promise<ProcessPage>, since: number, onPage: (page: ProcessPage) => void): Promise<void> {
  // A bound, in case a broken server keeps saying hasMore without moving on.
  for (let pages = 0; pages < 500; pages++) {
    const page = await list(since);
    onPage(page);
    const last = page.processes[page.processes.length - 1];
    if (!page.hasMore || !last || last.seq <= since) return;
    since = last.seq;
  }
}

/**
 * A team process's definition as the Process the views draw, or null when
 * it isn't a usable `augry/process@1` file. `version` is the server's, so the
 * editor, versions and diffs number it the way the team sees it.
 */
export function definitionToProcess(definition: unknown, version: number, fallbackId: string): Process | null {
  if (!definition || typeof definition !== "object" || Array.isArray(definition)) return null;
  // Starter templates installed from the server are in the cloud's lanes format.
  if (isV1(definition)) {
    const converted = processFromV1(definition, { id: fallbackId });
    return converted && { ...converted, version };
  }
  const d = definition as Record<string, unknown>;
  const process = { ...d, id: typeof d.id === "string" && d.id ? d.id : fallbackId, version } as unknown as Process;
  return validateProcess(process).length ? null : process;
}

/** "Created" for version 1, "Saved" after: the API keeps no message per version. */
function messageOf(version: number, fromTemplate: boolean): string {
  if (version === 1) return fromTemplate ? "Added from a template" : "Created";
  return "Saved";
}

export const parseTime = (iso: string): number => {
  const t = Date.parse(iso);
  return Number.isNaN(t) ? 0 : t;
};

/** A version entry for the latest content of a team process. */
export function teamVersion(tp: TeamProcess): ProcessVersion | null {
  const process = definitionToProcess(tp.definition, tp.version, tp.id);
  if (!process) return null;
  return {
    version: tp.version,
    message: messageOf(tp.version, !!tp.templateId),
    by: tp.updatedBy,
    byId: tp.updatedBy,
    at: parseTime(tp.updatedAt),
    hash: hashOf(process),
    process,
  };
}

/** A version from `GET …/versions/{v}`. */
export function fetchedVersion(info: ProcessVersionInfo & { definition: unknown }, id: string): ProcessVersion | null {
  const process = definitionToProcess(info.definition, info.version, id);
  if (!process) return null;
  return {
    version: info.version,
    message: messageOf(info.version, false),
    by: info.createdBy,
    byId: info.createdBy,
    at: parseTime(info.createdAt),
    hash: hashOf(process),
    process,
  };
}

/** Adds (or replaces) a version, oldest first. */
export function withVersion(record: ProcessRecord, version: ProcessVersion): ProcessRecord {
  const versions = [...record.versions.filter((v) => v.version !== version.version), version].sort((a, b) => a.version - b.version);
  return { ...record, versions };
}

/**
 * The team's records after the cache changed. A record with a draft keeps the
 * version it was started from, so a save still sends that base and a newer
 * version from a teammate comes back as a conflict instead of being lost.
 * Newest saves first, as the design orders them.
 */
export function syncTeamRecords(records: readonly ProcessRecord[], cache: TeamCache): ProcessRecord[] {
  const byId = new Map(records.map((r) => [r.id, r]));
  const out: { record: ProcessRecord; at: number }[] = [];
  for (const tp of Object.values(cache.processes)) {
    const existing = byId.get(tp.id);
    const at = parseTime(tp.updatedAt);
    if (existing && latest(existing).version === tp.version) {
      out.push({ record: existing, at });
      continue;
    }
    if (existing?.draft) {
      out.push({ record: existing, at });
      continue;
    }
    const version = teamVersion(tp);
    if (!version) continue;
    out.push({ record: existing ? withVersion(existing, version) : { id: tp.id, versions: [version], draft: null }, at });
  }
  return out.sort((a, b) => b.at - a.at || a.record.id.localeCompare(b.record.id)).map((x) => x.record);
}

// Names ------------------------------------------------------------------------

/** Member names by user id: the GitHub name, else the login. */
export function memberNames(members: readonly Member[]): Record<string, string> {
  return Object.fromEntries(members.map((m) => [m.user.id, m.user.name.trim() || m.user.login]));
}

export function firstName(name: string): string {
  return name.trim().split(/\s+/)[0] ?? name;
}

/** "2h ago", "yesterday", "3 days ago": when a team process was last saved. */
export function savedAgo(ts: number, now: number): string {
  const m = Math.max(0, Math.floor((now - ts) / 60000));
  if (m < 1) return "just now";
  if (m < 60) return `${m}m ago`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h}h ago`;
  return whenLabel(ts, now);
}

/** A card's status line: "Maya Chen saved v3 · 2h ago", "You saved v2 · 3 days ago". */
export function savedLine(who: string, version: number, at: number, now: number): string {
  return `${who} saved v${version} · ${savedAgo(at, now)}`;
}
