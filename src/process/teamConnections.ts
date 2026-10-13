/**
 * Connections with a team selected (Process board 11, team-connections and
 * team-connection-setup). Project management and Systems and data show the
 * team's own connections, one set of credentials for everyone; things on
 * this computer (folders, schedule, git, local webhook) stay each member's.
 *
 * A team connection is known by its `name` (unique among the team's live
 * ones, `${conn:<name>}` in a process), not its kind: a team can have two
 * GitHub accounts. The rules here mirror cloud epic 005 so a request the
 * server would refuse is explained before it is sent. Every call goes
 * through `enabled` (the server's `connections` feature); without it the
 * team sections read "Not set up for the team" and nothing is sent.
 */

import * as api from "../cloud/api";
import type { PublicUser, Role, SaveTeamConnectionRequest, TeamConnection, TeamConnectionEditors } from "../cloud/types";
import { connectionFields, CONNECTIONS } from "./connections";
import { uniqueId } from "./model";
import { scanSecrets, secretWhat } from "./secrets";
import type { UsedConnection } from "./share";
import type { Connection, ConnectionGroup } from "./types";

// Limits (epic 005 Decisions 3, 5, 6 and 11) ----------------------------------------

export const MAX_LIVE_CONNECTIONS = 100;
export const MAX_SETTINGS_KEYS = 50;
export const MAX_SETTING_VALUE_BYTES = 2000;
export const MAX_SETTINGS_BYTES = 16 * 1024;
export const MAX_SECRET_BYTES = 8192;

const NAME = /^[a-z0-9][a-z0-9-]{0,63}$/;
const SETTING_KEY = /^[A-Za-z0-9_.-]{1,64}$/;

export const NAME_RULE = "Names are 1 to 64 characters: lower case letters, digits and hyphens, starting with a letter or digit.";

/** A name the server takes. It never changes after create. */
export function isValidConnectionName(name: string): boolean {
  return NAME.test(name);
}

const bytes = (text: string) => new TextEncoder().encode(text).length;

/**
 * The size of the settings as the server measures it: Go's `json.Marshal`
 * of the map, keys sorted, which writes `<`, `>`, `&`, U+2028 and U+2029 as
 * six character escapes.
 */
export function settingsJsonBytes(settings: Record<string, string>): number {
  const sorted = Object.fromEntries(Object.keys(settings).sort().map((k) => [k, settings[k]]));
  const json = JSON.stringify(sorted);
  let extra = 0;
  for (const ch of json) {
    if (ch === "<" || ch === ">" || ch === "&") extra += 5;
    else if (ch === " " || ch === " ") extra += 3;
  }
  return bytes(json) + extra;
}

/**
 * Why the server would refuse these settings, or null. `labelOf` names a
 * key the way the panel shows it. Settings are plain text for every member,
 * so a value that looks like a token is refused here as the server would
 * (422 settings_contains_secret).
 */
export function settingsProblem(settings: Record<string, string>, labelOf: (key: string) => string = (k) => k, secretLabel = "the password or token field"): string | null {
  const keys = Object.keys(settings);
  if (keys.length > MAX_SETTINGS_KEYS) return `That's ${keys.length} settings. A connection can have at most ${MAX_SETTINGS_KEYS}.`;
  for (const key of keys) {
    if (!SETTING_KEY.test(key)) return "A setting's name isn't one the server takes: use 1 to 64 letters, digits, underscores, dots or hyphens.";
    if (bytes(settings[key]!) > MAX_SETTING_VALUE_BYTES) return `${labelOf(key)} is longer than ${MAX_SETTING_VALUE_BYTES.toLocaleString("en")} bytes. Shorten it.`;
  }
  if (settingsJsonBytes(settings) > MAX_SETTINGS_BYTES) return "The settings are larger than 16 KB together. Shorten them.";
  const found = scanSecrets(settings)[0];
  if (found) return secretInSettings(found.rule, labelOf(keyOfPath(found.path.startsWith("[") ? `$${found.path}` : `$.${found.path}`)), secretLabel);
  return null;
}

function secretInSettings(rule: string, label: string, secretLabel: string): string {
  return `${label} looks like ${secretWhat(rule)}. Every member can read settings, so put it in ${secretLabel} instead, where it is encrypted.`;
}

/** Why the server would refuse this secret, or null. Empty means none (or, on a save, remove it). */
export function secretProblem(secret: string): string | null {
  return bytes(secret) > MAX_SECRET_BYTES ? `It is longer than ${MAX_SECRET_BYTES.toLocaleString("en")} bytes, the most a connection can keep.` : null;
}

// Who may do what (epic 005 Decision 2) -------------------------------------------

/** Adding a connection: owners and admins only. */
export function canCreate(role: Role): boolean {
  return role !== "member";
}

/** Saving or removing one: owners and admins always, members when it allows everyone. */
export function canEdit(connection: TeamConnection, role: Role): boolean {
  return role !== "member" || connection.editors === "members";
}

/** Who can change it ("editors"): owners and admins only. */
export function canChangeEditors(role: Role): boolean {
  return role !== "member";
}

// Names and references -------------------------------------------------------------

/** A free name the API accepts, made from `wanted`: "github", then "github-2". */
export function connectionName(wanted: string, taken: readonly TeamConnection[]): string {
  const base = uniqueId(wanted, taken.filter((c) => !c.deleted).map((c) => c.name)).slice(0, 64).replace(/-+$/, "");
  return base || "connection";
}

/** How a process refers to it. */
export function connectionRef(name: string): string {
  return `\${conn:${name}}`;
}

/**
 * The team connection name a process's reference means: the reference
 * itself, or for `db.<name>` (how this app writes a database reference) the
 * part after `db.`. Null when no team connection could be called that.
 */
export function nameForRef(ref: string): string | null {
  if (isValidConnectionName(ref)) return ref;
  const db = ref.match(/^db\.(.+)$/);
  return db && isValidConnectionName(db[1]!) ? db[1]! : null;
}

/** The team's live connection a process's reference names, if any. */
export function teamConnectionForRef(ref: string, connections: readonly TeamConnection[]): TeamConnection | null {
  const name = nameForRef(ref);
  return (name && connections.find((c) => !c.deleted && c.name === name)) || null;
}

// The Connections view --------------------------------------------------------------

/** One card in a team's Connections. */
export interface TeamConnectionCard {
  /** Unique among the cards: the team connection's id, else the kind. */
  key: string;
  /** The catalog entry (kind, icon, description) this card is for. */
  local: Connection;
  /** The card's title. A team connection's carries its name. */
  title: string;
  /** The team's connection this card shows; null for a kind it hasn't set up. */
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

/**
 * The catalog entry for a kind. A kind this version of the app doesn't know
 * (another client made it) still gets a card, in Systems and data, with its
 * settings shown as they are.
 */
export function catalogOf(kind: string, local: readonly Connection[] = CONNECTIONS): Connection {
  return (
    local.find((c) => c.icon === kind && c.group !== "local") ?? {
      id: kind,
      group: "systems",
      icon: "plug",
      name: kind,
      on: false,
      status: "",
      desc: "A kind of connection this version of the app doesn't know.",
      used: "",
      settings: {},
    }
  );
}

/** Whether the app knows this kind, so the panel can show its own fields. */
export function isKnownKind(kind: string, local: readonly Connection[] = CONNECTIONS): boolean {
  return local.some((c) => c.icon === kind && c.group !== "local");
}

/**
 * The three sections in the design's order. Every live team connection gets
 * its own card, after the others of its kind by name; a kind the team has
 * none of reads "Not set up for the team". `team` is null when the team's
 * connections aren't available (no `connections` feature, or not loaded).
 */
export function teamConnectionSections(
  local: readonly Connection[],
  team: readonly TeamConnection[] | null,
  teamName: string,
  addedBy: (user: PublicUser) => string,
): TeamConnectionSection[] {
  const live = (team ?? []).filter((c) => !c.deleted).sort((a, b) => a.name.localeCompare(b.name));
  const card = (c: Connection, found: TeamConnection): TeamConnectionCard => ({
    key: found.id,
    local: c,
    title: `${c.name} · ${found.name}`,
    team: found,
    mine: false,
    on: true,
    status: sharedDetail(found),
    used: `Added by ${addedBy(found.createdBy)}`,
  });
  const shared = (group: ConnectionGroup): TeamConnectionCard[] => {
    const cards = local
      .filter((c) => c.group === group)
      .flatMap((c) => {
        const found = live.filter((t) => t.kind === c.icon);
        return found.length
          ? found.map((t) => card(c, t))
          : [{ key: c.icon, local: c, title: c.name, team: null, mine: false, on: false, status: "Not set up for the team", used: "" }];
      });
    // Kinds this app doesn't know go last in Systems and data.
    if (group === "systems") for (const t of live) if (!isKnownKind(t.kind, local)) cards.push(card(catalogOf(t.kind, local), t));
    return cards;
  };
  return [
    { id: "work", title: "Project management", hint: `Shared with ${teamName} · one sign-in for the whole team`, cards: shared("work") },
    { id: "systems", title: "Systems and data", hint: `Shared with ${teamName} · read only unless the team allows writes`, cards: shared("systems") },
    {
      id: "local",
      title: "On this computer",
      hint: "Not shared. Each member sets these up on their own computer",
      cards: local
        .filter((c) => c.group === "local")
        .map((c) => ({ key: c.id, local: c, title: c.name, team: null, mine: true, on: c.on, status: c.status, used: "Yours only" })),
    },
  ];
}

// Writing ---------------------------------------------------------------------------

export interface ConnectionDraft {
  settings: Record<string, string>;
  /**
   * Undefined keeps the stored secret, "" removes it, anything else replaces
   * it. On a create, undefined and "" both mean none.
   */
  secret?: string;
  editors: TeamConnectionEditors;
}

/**
 * The save body. `editors` goes only when it changed (a member may never
 * change it, and leaving it out can't trip that rule); `secret` follows the
 * draft's keep, remove or replace.
 */
export function saveRequest(existing: TeamConnection, draft: ConnectionDraft): SaveTeamConnectionRequest {
  const body: SaveTeamConnectionRequest = { settings: draft.settings, baseVersion: existing.version };
  if (draft.secret !== undefined) body.secret = draft.secret;
  if (draft.editors !== existing.editors) body.editors = draft.editors;
  return body;
}

/** The settings a local connection shares with a team: never a field that holds a secret. */
export function sharedSettings(local: Connection): Record<string, string> {
  const secret = new Set(connectionFields(local).filter((f) => f.type === "secret").map((f) => f.key));
  return Object.fromEntries(Object.entries(local.settings).filter(([k]) => !secret.has(k)));
}

/** What the share dialog can do about one connection the process uses. */
export type ShareRow =
  /** The team's connections aren't known (no feature, loading, or the load failed). */
  | { state: "unknown" }
  | { state: "has"; connection: TeamConnection }
  | {
      state: "missing";
      /** The name it would be shared as, when the reference is one a team connection can have. */
      name: string | null;
      /** True when "Share my connection" can be ticked. */
      canShare: boolean;
      /** Why it can't be shared from here, when there is a local one but it can't. */
      why: string | null;
    };

export function shareRowOf(used: UsedConnection, team: readonly TeamConnection[] | null, role: Role, teamName: string): ShareRow {
  if (!team) return { state: "unknown" };
  const found = teamConnectionForRef(used.ref, team);
  if (found) return { state: "has", connection: found };
  const name = nameForRef(used.ref);
  if (!used.local || !used.kind) return { state: "missing", name, canShare: false, why: null };
  if (!canCreate(role)) {
    return { state: "missing", name, canShare: false, why: `Only owners and admins can add connections to ${teamName}. Ask one to add ${name ?? used.ref} in Connections.` };
  }
  if (!name) {
    return {
      state: "missing",
      name,
      canShare: false,
      why: `The file calls it ${used.ref}, which a team connection can't be called. ${NAME_RULE}`,
    };
  }
  return { state: "missing", name, canShare: true, why: null };
}

// Errors ----------------------------------------------------------------------------

export interface ErrorContext {
  action: "load" | "create" | "save" | "remove";
  teamName: string;
  /** The connection's name, for a create's name clash. */
  name?: string;
  /** A setting's label in the panel, for the 422. */
  labelOf?(key: string): string;
  /** Where a token belongs instead of settings: "the Password field". */
  secretLabel?: string;
}

/** `$.host` or `$["a.b"]` from settings_contains_secret, as the key. */
function keyOfPath(path: string): string {
  const quoted = path.match(/^\$\["(.*)"\]$/);
  if (quoted) return quoted[1]!.replace(/\\(.)/g, "$1");
  return path.replace(/^\$\.?/, "");
}

/** What a failed connections call says, by its status and the server's message. */
export function connectionErrorMessage(error: unknown, ctx: ErrorContext): string {
  const { action, teamName } = ctx;
  if (!api.isCloudError(error)) return `Couldn't ${verb(action)} ${teamName}. Try again in a moment.`;
  const message = error.message;
  if (error.status === 503 && /not configured/i.test(message)) return `Team connections aren't switched on for this server yet, so nothing was ${done(action)}.`;
  if (error.offline) return action === "load" ? `You're offline. ${teamName}'s connections show once you're back online.` : `You're offline. Try again once you're back online.`;
  switch (error.status) {
    case 400:
      if (/already has \d+ connections/.test(message)) return `${teamName} already has ${MAX_LIVE_CONNECTIONS} connections, the most a team can have. Remove one first.`;
      if (/^name must/.test(message)) return NAME_RULE;
      if (/^secret must/.test(message)) return `The password or token is longer than ${MAX_SECRET_BYTES.toLocaleString("en")} bytes, the most a connection can keep.`;
      if (/^settings has (\d+) keys/.test(message)) return `A connection can have at most ${MAX_SETTINGS_KEYS} settings.`;
      if (/settings value is longer/.test(message)) return `A setting is longer than ${MAX_SETTING_VALUE_BYTES.toLocaleString("en")} bytes. Shorten it.`;
      if (/settings are larger/.test(message)) return "The settings are larger than 16 KB together. Shorten them.";
      if (/settings key is invalid/.test(message)) return "A setting's name isn't one the server takes: use 1 to 64 letters, digits, underscores, dots or hyphens.";
      return `${teamName} didn't take it: ${message}.`;
    case 403:
      if (action === "create") return `Only owners and admins can add connections to ${teamName}.`;
      if (/who can edit/.test(message)) return "Only owners and admins can change who can change it.";
      return `Only owners and admins can ${action === "remove" ? "remove" : "change"} this connection.`;
    case 404:
      return action === "load" || action === "create" ? `${teamName} isn't there any more, or you're no longer in it.` : "This connection isn't there any more.";
    case 409:
      if (action === "create") return `${teamName} already has a connection named ${ctx.name ?? "that"}. Pick another name.`;
      if ((error.current as TeamConnection | undefined)?.deleted) return "Someone removed it a moment ago.";
      return action === "remove" ? "Someone changed it a moment ago. Look it over, then remove it again." : "Someone changed it a moment ago. Look it over and save again.";
    case 422: {
      const m = message.match(/^settings_contains_secret:\s*([\w-]+)\s+at\s+(\S+)/);
      if (m) return secretInSettings(m[1]!, (ctx.labelOf ?? ((k: string) => k))(keyOfPath(m[2]!)), ctx.secretLabel ?? "the password or token field");
      return `${teamName} didn't take it: ${message}.`;
    }
    case 429:
      return "Too many tries. Wait a minute, then try again.";
  }
  return `Couldn't ${verb(action)} ${teamName}. Try again in a moment.`;
}

function verb(action: ErrorContext["action"]): string {
  return { load: "load the connections of", create: "add it to", save: "save it to", remove: "remove it from" }[action];
}

function done(action: ErrorContext["action"]): string {
  return { load: "loaded", create: "added", save: "saved", remove: "removed" }[action];
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

function off(): never {
  throw new Error("Team connections are not available");
}

/** Adds a team connection. Owners and admins only. Throws when the feature is off. */
export async function createTeamConnection(
  teamId: string,
  enabled: boolean,
  connection: { name: string; kind: string } & ConnectionDraft,
): Promise<TeamConnection> {
  if (!enabled) off();
  const { name, kind, settings, secret, editors } = connection;
  return api.createTeamConnection(teamId, { name, kind, settings, editors, ...(secret ? { secret } : {}) });
}

/** Saves a team connection as its next version. Throws when the feature is off. */
export async function saveTeamConnection(teamId: string, enabled: boolean, existing: TeamConnection, draft: ConnectionDraft): Promise<TeamConnection> {
  if (!enabled) off();
  return api.saveTeamConnection(teamId, existing.id, saveRequest(existing, draft));
}

export async function removeTeamConnection(teamId: string, enabled: boolean, connection: TeamConnection): Promise<void> {
  if (!enabled) off();
  await api.deleteTeamConnection(teamId, connection.id, connection.version);
}
