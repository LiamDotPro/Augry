/**
 * What "Share to team…" (Process board 11, share-to-team) lists under
 * "Connections it uses": every connection the file refers to by name
 * (`${conn:<name>}`), plus the ones its start and steps imply (a ticket start
 * reads its tracker, `gh` commands use GitHub). Only connections a team can
 * hold count; folders, schedules, git and local webhooks stay on each
 * computer.
 */

import type { Connection, Process } from "./types";

export interface UsedConnection {
  /** The name the file uses: "github", "db.staging". */
  ref: string;
  /** "GitHub Issues", "PostgreSQL · staging". */
  label: string;
  /** The matching connection on this computer, when there is one. */
  local?: Connection;
  /** The kind the team would store it as ("github", "postgresql"), when known. */
  kind?: string;
}

const CONN_REF = /\$\{conn:([A-Za-z0-9_.-]+)\}/g;

/** Kinds a team can hold (Project management, Systems and data). */
const TEAM_GROUPS = new Set(["work", "systems"]);

/** Short names a file may use for a kind. */
const ALIASES: Record<string, string> = { gh: "github", pg: "postgresql", postgres: "postgresql", db: "postgresql" };

function strings(value: unknown, out: string[] = []): string[] {
  if (typeof value === "string") out.push(value);
  else if (Array.isArray(value)) value.forEach((v) => strings(v, out));
  else if (value && typeof value === "object") Object.values(value).forEach((v) => strings(v, out));
  return out;
}

/** Every name the file refers to, in order of first use. */
export function connectionRefs(process: Process): string[] {
  const refs: string[] = [];
  const add = (ref: string) => {
    const r = ref.trim();
    if (r && !refs.includes(r)) refs.push(r);
  };
  if (process.start.on === "ticket" && typeof process.start.source === "string") add(process.start.source);
  for (const step of process.steps) {
    for (const key of ["connection", "conn"]) {
      const v = step[key];
      if (typeof v === "string") add(v);
    }
    if (typeof step.run === "string" && /(^|[\s;&|(])gh\s/.test(step.run)) add("github");
  }
  for (const text of strings(process)) for (const m of text.matchAll(CONN_REF)) add(m[1]!);
  return refs;
}

function findLocal(ref: string, connections: readonly Connection[]): { local?: Connection; rest: string } {
  const lower = ref.toLowerCase();
  const byName = connections.find((c) => c.settings.name?.trim().toLowerCase() === lower);
  if (byName) return { local: byName, rest: "" };
  const [head, ...tail] = lower.split(".");
  const id = ALIASES[head!] ?? head!;
  return { local: connections.find((c) => c.id === id || c.icon === id), rest: tail.join(".") };
}

/** The connections a process uses that a team could share, with what each is on this computer. */
export function connectionsUsed(process: Process, connections: readonly Connection[]): UsedConnection[] {
  const out: UsedConnection[] = [];
  for (const ref of connectionRefs(process)) {
    const { local, rest } = findLocal(ref, connections);
    if (local && !TEAM_GROUPS.has(local.group)) continue;
    const label = local ? (rest ? `${local.name} · ${rest}` : local.name) : ref;
    if (out.some((u) => u.label === label)) continue;
    out.push({ ref, label, local, kind: local?.icon });
  }
  return out;
}
