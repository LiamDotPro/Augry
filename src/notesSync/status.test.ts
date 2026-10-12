import { describe, expect, it } from "vitest";
import { clock, conflictLead, footerCopy, savedStatus } from "./status";

const at = new Date(2026, 9, 12, 10, 42).getTime();
const base = { conflicts: 0, offline: false, waiting: 0, syncedAt: at, personal: true, teams: [{ name: "Northwind", members: 6 }] };

describe("footerCopy", () => {
  it("says when it synced and who each space reaches", () => {
    expect(footerCopy(base)).toEqual({
      tone: "ok",
      line: "Synced · 10:42",
      text: "Personal syncs to your devices. Northwind is shared with its 6 members.",
    });
  });

  it("puts conflicts first", () => {
    expect(footerCopy({ ...base, conflicts: 1, offline: true })).toEqual({
      tone: "warn",
      line: "1 note needs you",
      text: "Everything else is synced · 10:42",
    });
  });

  it("counts what waits while offline", () => {
    expect(footerCopy({ ...base, offline: true, waiting: 3 })).toEqual({
      tone: "ring",
      line: "Offline · 3 changes waiting",
      text: "Saved on this computer. They sync when you’re back online.",
    });
  });

  it("leaves out Personal when only a team syncs", () => {
    expect(footerCopy({ ...base, personal: false, teams: [{ name: "Acme", members: 1 }] }).text).toBe(
      "Acme is shared with its 1 member.",
    );
  });
});

describe("savedStatus", () => {
  it("covers synced, offline and conflicted notes", () => {
    const ok = { conflict: false, refused: false, offline: false, waiting: false };
    expect(savedStatus(ok).text).toBe("Saved · synced");
    expect(savedStatus({ ...ok, offline: true }).text).toBe("Saved on this computer · will sync");
    expect(savedStatus({ ...ok, conflict: true, offline: true }).text).toBe("Saved on this computer · not synced");
  });
});

describe("conflictLead", () => {
  it("names who saved it, and starts a sentence with a capital", () => {
    expect(conflictLead("Maya Chen")).toBe(
      "Maya Chen saved this note while you were editing. Yours is here; theirs is kept as",
    );
    expect(conflictLead("another device")).toMatch(/^Another device saved/);
  });

  it("formats the time on a 24-hour clock", () => {
    expect(clock(new Date(2026, 0, 1, 9, 5).getTime())).toBe("09:05");
  });
});
