/**
 * Connections with a team selected (Process board 11, team-connections and
 * team-connection-setup). Project management and Systems and data show the
 * team's own connections, one set of credentials for everyone; things on
 * this computer (folders, schedule, git, local webhook) stay each member's.
 *
 * The team connections API is cloud epic 005 and isn't built yet, so every
 * call here goes through `enabled` (the server's `connections` feature).
 * Without it the team sections read "Not set up for the team" and nothing is
 * sent.
 */

import * as api from "../cloud/api";
import type { PublicUser, TeamConnection } from "../cloud/types";
import { uniqueId } from "./model";
import type { Connection, ConnectionGroup } from "./types";

/** One card in a team's Connections. */
export interface TeamConnectionCard {
  /** The catalog entry (kind, icon, description) this card is for. */
  local: Connection;
  /** The team's connection of this kind, if it has one. */
  team: TeamConnection | null;
  /** Local connections: kept as they are, marked "Yours only". */
  mine: boolean;
  on: boolean;
  status: string;
  used: string;
}

export interface TeamConnectionSection {
  id: ConnectionGroup;
  title: string;
  hint: string;
  cards: TeamConnectionCard[];
}

/** The card's detail after "Shared ·": the account, else the host without its port. */
export function sharedDetail(connection: TeamConnection): string {
  const s = connection.settings;
  const detail = s.account?.trim() || s.host?.trim().replace(/:\d+$/, "") || s.name?.trim() || connection.name;
  return `Shared · ${detail}`;
}

/** The team's live connection of a kind; the oldest wins if there are several. */
export function teamConnectionOf(kind: string, connections: readonly TeamConnection[]): TeamConnection | null {
  return connections.filter((c) => !c.deleted && c.kind === kind).sort((a, b) => a.seq - b.seq)[0] ?? null;
}

/**
 * The three sections in the design's order. `team` is null when the team's
 * connections aren't available (no `connections` feature, or not loaded):
 * every shared kind then reads "Not set up for the team".
 */
export function teamConnectionSections(
  local: readonly Connection[],
  team: readonly TeamConnection[] | null,
  teamName: string,
  addedBy: (user: PublicUser) => string,
): TeamConnectionSection[] {
  const shared = (group: ConnectionGroup): TeamConnectionCard[] =>
    local
      .filter((c) => c.group === group)
      .map((c) => {
        const found = team ? teamConnectionOf(c.icon, team) : null;
        return found
          ? { local: c, team: found, mine: false, on: true, status: sharedDetail(found), used: `Added by ${addedBy(found.createdBy)}` }
          : { local: c, team: null, mine: false, on: false, status: "Not set up for the team", used: "" };
      });
  return [
    { id: "work", title: "Project management", hint: `Shared with ${teamName} · one sign-in for the whole team`, cards: shared("work") },
    { id: "systems", title: "Systems and data", hint: `Shared with ${teamName} · read only unless the team allows writes`, cards: shared("systems") },
    {
      id: "local",
      title: "On this computer",
      hint: "Not shared. Each member sets these up on their own computer",
      cards: local.filter((c) => c.group === "local").map((c) => ({ local: c, team: null, mine: true, on: c.on, status: c.status, used: "Yours only" })),
    },
  ];
}

/** A name the API accepts (lower case `[a-z0-9-]`, 1 to 64), unique in the team. */
export function connectionName(wanted: string, taken: readonly TeamConnection[]): string {
  const base = uniqueId(wanted, taken.filter((c) => !c.deleted).map((c) => c.name)).slice(0, 64).replace(/-+$/, "");
  return base || "connection";
}

/** How a process refers to it: `db.staging` for a database, the name otherwise. */
export function referenceOf(group: ConnectionGroup, name: string): string {
  return group === "systems" ? `db.${name}` : name;
}

/** Whether you can change it: owners and admins always, members when it allows everyone. */
export function canEdit(connection: TeamConnection | null, role: "owner" | "admin" | "member"): boolean {
  if (role !== "member") return true;
  return connection?.editors === "members";
}

// Calls, only with the feature ------------------------------------------------------

/** Every live team connection, paging through the feed. Null without the feature. */
export async function loadTeamConnections(teamId: string, enabled: boolean): Promise<TeamConnection[] | null> {
  if (!enabled) return null;
  const byId = new Map<string, TeamConnection>();
  let since = 0;
  for (let pages = 0; pages < 100; pages++) {
    const page = await api.listTeamConnections(teamId, since);
    for (const c of page.connections) {
      if (c.deleted) byId.delete(c.id);
      else byId.set(c.id, c);
    }
    const last = page.connections[page.connections.length - 1];
    if (!page.hasMore || !last || last.seq <= since) break;
    since = last.seq;
  }
  return [...byId.values()];
}

export interface ConnectionDraft {
  settings: Record<string, string>;
  /** Typed in this session only; left out to keep the stored one. */
  secret?: string;
  editors: "admins" | "members";
}

/** Creates or saves a team connection. Throws when the feature is off. */
export async function writeTeamConnection(
  teamId: string,
  enabled: boolean,
  kind: string,
  existing: TeamConnection | null,
  draft: ConnectionDraft,
  taken: readonly TeamConnection[],
): Promise<TeamConnection> {
  if (!enabled) throw new Error("Team connections are not available");
  const secret = draft.secret?.trim() ? draft.secret : undefined;
  if (existing) {
    return api.saveTeamConnection(teamId, existing.id, { settings: draft.settings, secret, editors: draft.editors, baseVersion: existing.version });
  }
  const name = connectionName(draft.settings.name || kind, taken);
  return api.createTeamConnection(teamId, { name, kind, settings: draft.settings, secret, editors: draft.editors });
}

export async function removeTeamConnection(teamId: string, enabled: boolean, connection: TeamConnection): Promise<void> {
  if (!enabled) throw new Error("Team connections are not available");
  await api.deleteTeamConnection(teamId, connection.id, connection.version);
}
