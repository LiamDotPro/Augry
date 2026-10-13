/**
 * Wording and rules for the team dialogs (App Shell board 08): what each API
 * error says, who can change what, and the sentences built from counts. Kept
 * apart from the components so it can be tested without a window.
 */
import type { Invite, Member, ProcessPage, Role, TeamConnectionPage } from "./types";

// Names ------------------------------------------------------------------------

export const TEAM_NAME_MAX = 80;

/** The API trims names and takes 1 to 80 characters. */
export function isValidTeamName(name: string): boolean {
  const trimmed = name.trim();
  return trimmed.length >= 1 && trimmed.length <= TEAM_NAME_MAX;
}

export const TEAM_NAME_RULE = "Team names are 1 to 80 characters.";

// Roles --------------------------------------------------------------------------

const ROLE_LABEL: Record<Role, string> = { owner: "Owner", admin: "Admin", member: "Member" };
const ROLE_NOUN: Record<Role, string> = { owner: "an owner", admin: "an admin", member: "a member" };

/** "Owner", "Admin", "Member": the dropdown and the invite card. */
export function roleLabel(role: Role): string {
  return ROLE_LABEL[role];
}

/** "an owner", "an admin", "a member": inside sentences. */
export function roleNoun(role: Role): string {
  return ROLE_NOUN[role];
}

/** Owners and admins invite, rename and change roles; members only look. */
export function canManage(role: Role): boolean {
  return role !== "member";
}

/**
 * The roles a member's dropdown offers. Only an owner can make someone an
 * owner; an admin still sees an owner's current role, but can't pick it.
 */
export function roleChoices(myRole: Role): Role[] {
  return myRole === "owner" ? ["owner", "admin", "member"] : ["admin", "member"];
}

/** Whether my role lets me change this member's role at all. */
export function canChangeRole(myRole: Role, member: Member): boolean {
  if (myRole === "owner") return true;
  return myRole === "admin" && member.role !== "owner";
}

/** The × on a member's row. Never on your own row: that is Leave team. */
export function canRemove(myRole: Role, myId: string | undefined, member: Member): boolean {
  if (member.user.id === myId) return false;
  return canChangeRole(myRole, member);
}

// Errors -------------------------------------------------------------------------

const OFFLINE = (status: number) => status === 0 || status >= 500;

/** team-join-error, by the accept call's status. */
export function joinErrorMessage(status: number): string {
  if (status === 410) return "This invite has expired or was already used. Ask whoever sent it for a new one.";
  if (status === 404) return "That code isn't right. Check it and try again.";
  if (status === 409) return "You're already in this team.";
  if (status === 429) return "Too many tries. Wait a minute, then try again.";
  if (OFFLINE(status)) return "You're offline. Join when you're back online.";
  return "Couldn't join the team. Try again in a moment.";
}

/** team-create: the create call's status. */
export function createErrorMessage(status: number): string {
  if (status === 400) return TEAM_NAME_RULE;
  if (status === 403) return "Your account can't create teams right now.";
  if (status === 429) return "Too many tries. Wait a minute, then try again.";
  if (OFFLINE(status)) return "You're offline. Create the team when you're back online.";
  return "Couldn't create the team. Try again in a moment.";
}

/** team-settings: renaming, roles, removing, inviting, leaving and deleting. */
export function settingsErrorMessage(status: number): string {
  if (status === 409) return "A team needs at least one owner. Make someone else an owner first.";
  if (status === 403) return "Your role in this team doesn't allow that. Ask an owner if it needs doing.";
  if (status === 404) return "This team isn't there any more, or you're no longer in it.";
  if (status === 400) return TEAM_NAME_RULE;
  if (status === 429) return "Too many tries. Wait a minute, then try again.";
  if (OFFLINE(status)) return "You're offline. Changes can't be saved until you're back online.";
  return "That didn't work. Try again in a moment.";
}

// Counts -------------------------------------------------------------------------

function plural(count: number, one: string, many: string): string {
  return `${count} ${count === 1 ? one : many}`;
}

/** team-joined, the Personal column. `count` is undefined until the Process tab has counted. */
export function personalSummary(count: number | undefined): string {
  if (count === undefined) return "Stays on this computer. Your processes are not shared.";
  if (count === 0) return "Stays on this computer. Processes you make there are not shared.";
  return `Stays on this computer. Your ${plural(count, "process is", "processes are")} not shared.`;
}

export interface JoinedCounts {
  /** Null while loading or when it couldn't be fetched. */
  members: number | null;
  /** Null when unknown or when the server has no team processes. */
  processes: number | null;
  /** Null when unknown or when the server has no team connections. */
  connections: number | null;
  role: Role;
}

/**
 * team-joined, the team column: "6 members share its 4 processes and 3
 * connections. You joined as a member." Leaves out whatever isn't known.
 */
export function joinedSummary({ members, processes, connections, role }: JoinedCounts): string {
  const joined = `You joined as ${roleNoun(role)}.`;
  if (members === null) return joined;
  const people = plural(members, "member", "members");
  const shared: string[] = [];
  if (processes !== null && processes > 0) shared.push(plural(processes, "process", "processes"));
  if (connections !== null && connections > 0) shared.push(plural(connections, "connection", "connections"));
  if (shared.length === 0) return `It has ${people}. ${joined}`;
  return `${people} ${members === 1 ? "shares" : "share"} its ${shared.join(" and ")}. ${joined}`;
}

/**
 * How many live items a change feed holds now, paging through it from the
 * start. The feed includes tombstones and can list an item again after a
 * later change, so the latest entry per id decides.
 */
async function countLive<T extends { id: string; seq: number; deleted: boolean }>(
  fetchPage: (since: number) => Promise<{ items: T[]; hasMore: boolean }>,
): Promise<number> {
  const latest = new Map<string, { seq: number; deleted: boolean }>();
  let since = 0;
  for (;;) {
    const page = await fetchPage(since);
    for (const item of page.items) {
      const seen = latest.get(item.id);
      if (!seen || item.seq >= seen.seq) latest.set(item.id, { seq: item.seq, deleted: item.deleted });
    }
    const last = page.items[page.items.length - 1];
    if (!page.hasMore || !last || last.seq <= since) break;
    since = last.seq;
  }
  let live = 0;
  for (const entry of latest.values()) if (!entry.deleted) live += 1;
  return live;
}

/** How many processes a team has now. */
export function countLiveProcesses(fetchPage: (since: number) => Promise<ProcessPage>): Promise<number> {
  return countLive(async (since) => {
    const page = await fetchPage(since);
    return { items: page.processes, hasMore: page.hasMore };
  });
}

/** How many connections a team has now (the `connections` feature). */
export function countLiveConnections(fetchPage: (since: number) => Promise<TeamConnectionPage>): Promise<number> {
  return countLive(async (since) => {
    const page = await fetchPage(since);
    return { items: page.connections, hasMore: page.hasMore };
  });
}

// Invites ------------------------------------------------------------------------

/** "Expires after", in hours as the API takes them. 7 days is the default. */
export const INVITE_EXPIRY_OPTIONS = [
  { hours: 24, label: "1 day" },
  { hours: 168, label: "7 days" },
  { hours: 720, label: "30 days" },
] as const;

export const DEFAULT_INVITE_HOURS = 168;

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/** "18 Oct" in local time; the year is added when it isn't this year. */
export function formatShortDate(iso: string, now: Date = new Date()): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return "";
  const day = `${date.getDate()} ${MONTHS[date.getMonth()]}`;
  return date.getFullYear() === now.getFullYear() ? day : `${day} ${date.getFullYear()}`;
}

/** The line under a new invite: "Works once · joins as Member · expires 18 Oct". */
export function inviteSummary(invite: Pick<Invite, "role" | "expiresAt">, now: Date = new Date()): string {
  return `Works once · joins as ${roleLabel(invite.role)} · expires ${formatShortDate(invite.expiresAt, now)}`;
}

/** The link as the card shows it, without the scheme: augry.app/join/K7M2P-9XQ4R. */
export function displayInviteUrl(url: string): string {
  return url.replace(/^https?:\/\//i, "");
}
