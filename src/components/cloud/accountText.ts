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

/** The notes spaces that sync, as stored in the "notesSync" choice, with their lastSyncedAt keys. */
function syncedSpaces(choice: NotesSyncChoice | null, teams: Team[]): { key: string; name: string }[] {
  if (!choice) return [];
  return [
    ...(choice.personal ? [{ key: "notes:personal", name: "Personal" }] : []),
    ...teams.filter((team) => choice.teams.includes(team.id)).map((team) => ({ key: `notes:${team.id}`, name: team.name })),
  ];
}

/** "Personal and Northwind": what syncs. Null means sync is off. */
export function notesSyncNames(choice: NotesSyncChoice | null, teams: Team[]): string | null {
  const spaces = syncedSpaces(choice, teams);
  return spaces.length > 0 ? listNames(spaces.map((space) => space.name)) : null;
}

/** When any synced space last synced, epoch ms; null before the first sync. */
export function notesSyncedAt(
  choice: NotesSyncChoice | null,
  teams: Team[],
  lastSyncedAt: Record<string, number>,
): number | null {
  const times = syncedSpaces(choice, teams)
    .map((space) => lastSyncedAt[space.key])
    .filter((at): at is number => typeof at === "number");
  return times.length > 0 ? Math.max(...times) : null;
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
