/**
 * Wording for the account screens (App Shell boards 01, 07 and 08): device
 * rows, the notes sync summary, process counts in the account menu. Pure, so
 * it is tested on its own (accountText.test.ts).
 */
import type { NotesSyncChoice, Role, Team } from "../../cloud/types";

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const DAY_MS = 24 * 60 * 60 * 1000;

/** "11 Oct", with the year when it isn't this year: "11 Oct 2025". */
export function shortDate(date: Date, now: Date): string {
  const day = `${date.getDate()} ${MONTHS[date.getMonth()]}`;
  return date.getFullYear() === now.getFullYear() ? day : `${day} ${date.getFullYear()}`;
}

/** Whole calendar days from `date` to `now`, in local time. */
function daysAgo(date: Date, now: Date): number {
  const start = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
  return Math.round((start(now) - start(date)) / DAY_MS);
}

/** A signed-in device's line: "Signed in 11 Oct · used today", "Signed in 1 Oct · last used 10 Oct". */
export function deviceUsage(createdAt: string, lastUsedAt: string, now: Date): string {
  const created = new Date(createdAt);
  const used = new Date(lastUsedAt);
  const days = daysAgo(used, now);
  const usage = days <= 0 ? "used today" : days === 1 ? "used yesterday" : `last used ${shortDate(used, now)}`;
  return `Signed in ${shortDate(created, now)} · ${usage}`;
}

/** "Signed in on 2 devices". */
export function deviceCount(count: number): string {
  return `Signed in on ${count} ${count === 1 ? "device" : "devices"}`;
}

/** 24-hour clock, the way the design shows sync times: "10:42". */
export function clockTime(at: number): string {
  const date = new Date(at);
  return `${String(date.getHours()).padStart(2, "0")}:${String(date.getMinutes()).padStart(2, "0")}`;
}

/** "A", "A and B", "A, B and C". */
export function listNames(names: string[]): string {
  if (names.length <= 1) return names[0] ?? "";
  return `${names.slice(0, -1).join(", ")} and ${names[names.length - 1]}`;
}

/**
 * Settings' Notes sync row: "Personal and Northwind · synced 10:42", from
 * the sync choice, the teams and when each notes space last synced
 * ("notes:personal", "notes:<teamId>"). Null means sync is off.
 */
export function notesSyncSummary(
  choice: NotesSyncChoice | null,
  teams: Team[],
  lastSyncedAt: Record<string, number>,
): string | null {
  if (!choice) return null;
  const keys: string[] = [];
  const names: string[] = [];
  if (choice.personal) {
    keys.push("notes:personal");
    names.push("Personal");
  }
  for (const team of teams) {
    if (!choice.teams.includes(team.id)) continue;
    keys.push(`notes:${team.id}`);
    names.push(team.name);
  }
  if (names.length === 0) return null;
  const times = keys.map((key) => lastSyncedAt[key]).filter((at): at is number => typeof at === "number");
  const synced = times.length > 0 ? ` · synced ${clockTime(Math.max(...times))}` : "";
  return `${listNames(names)}${synced}`;
}

/** "1 process", "4 processes". */
export function processCount(count: number): string {
  return `${count} ${count === 1 ? "process" : "processes"}`;
}

/** The account menu's second line: "On this computer · 3 processes", "Team · owner · 4 processes". */
export function spaceDetail(space: { role: Role } | "personal", count: number | undefined): string {
  const parts = space === "personal" ? ["On this computer"] : ["Team", space.role];
  if (count !== undefined) parts.push(processCount(count));
  return parts.join(" · ");
}

/** How long a device code works, in whole minutes (GitHub's are 15). */
export function codeMinutes(expiresAt: number, now: number): number {
  return Math.max(1, Math.round((expiresAt - now) / 60000));
}
