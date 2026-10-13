/**
 * The words notes sync shows (Notes board 02): the tree footer, the editor's
 * save status and the conflict banner. Pure, so the copy is tested.
 */

/** 24-hour "10:42". */
export function clock(at: number): string {
  const date = new Date(at);
  return `${String(date.getHours()).padStart(2, "0")}:${String(date.getMinutes()).padStart(2, "0")}`;
}

export type SyncTone = "ok" | "warn" | "ring" | "idle";

export interface FooterInput {
  conflicts: number;
  offline: boolean;
  /** The server asked to slow down: changes wait and sync tries again shortly. */
  retrying: boolean;
  waiting: number;
  syncedAt: number | null;
  personal: boolean;
  /** Synced teams, with their member count once known. */
  teams: { name: string; members: number | null }[];
}

/** Who each synced space reaches: "Personal syncs to your devices. Northwind is shared with its 6 members." */
export function reachText(personal: boolean, teams: FooterInput["teams"]): string {
  const parts = personal ? ["Personal syncs to your devices."] : [];
  for (const team of teams) {
    parts.push(
      team.members === null
        ? `${team.name} is shared with its members.`
        : `${team.name} is shared with its ${team.members} ${team.members === 1 ? "member" : "members"}.`,
    );
  }
  return parts.join(" ");
}

/** The tree footer: a dot, a line and a sentence under it. */
export function footerCopy(input: FooterInput): { tone: SyncTone; line: string; text: string } {
  if (input.conflicts > 0) {
    const n = input.conflicts;
    return {
      tone: "warn",
      line: n === 1 ? "1 note needs you" : `${n} notes need you`,
      text: input.syncedAt === null ? "Everything else is synced" : `Everything else is synced · ${clock(input.syncedAt)}`,
    };
  }
  if (input.offline) {
    const n = input.waiting;
    return {
      tone: "ring",
      line: n === 0 ? "Offline" : `Offline · ${n} ${n === 1 ? "change" : "changes"} waiting`,
      text: "Saved on this computer. They sync when you’re back online.",
    };
  }
  if (input.retrying && input.waiting > 0) {
    const n = input.waiting;
    return {
      tone: "ring",
      line: `${n} ${n === 1 ? "change" : "changes"} waiting`,
      text: "The server is busy. Saved on this computer; sync tries again in a moment.",
    };
  }
  return {
    tone: input.syncedAt === null ? "idle" : "ok",
    line: input.syncedAt === null ? "Syncing…" : `Synced · ${clock(input.syncedAt)}`,
    text: reachText(input.personal, input.teams),
  };
}

export interface NoteSyncInput {
  conflict: boolean;
  refused: boolean;
  offline: boolean;
  /** Its change hasn't reached the server yet. */
  waiting: boolean;
}

/** The editor's status for a saved note in a synced space. */
export function savedStatus(input: NoteSyncInput): { text: string; tone: SyncTone } {
  if (input.conflict || input.refused) return { text: "Saved on this computer · not synced", tone: "warn" };
  if (input.offline) return { text: "Saved on this computer · will sync", tone: "idle" };
  if (input.waiting) return { text: "Saved", tone: "ok" };
  return { text: "Saved · synced", tone: "ok" };
}

/** "Maya Chen saved this note while you were editing. Yours is here; theirs is kept as" (then the copy's name). */
export function conflictLead(theirName: string): string {
  const who = theirName.charAt(0).toUpperCase() + theirName.slice(1);
  return `${who} saved this note while you were editing. Yours is here; theirs is kept as`;
}
