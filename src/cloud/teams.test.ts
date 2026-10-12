import { describe, expect, it } from "vitest";
import {
  canChangeRole,
  canRemove,
  countLiveProcesses,
  displayInviteUrl,
  formatShortDate,
  inviteSummary,
  isValidTeamName,
  joinErrorMessage,
  joinedSummary,
  personalSummary,
  roleChoices,
  settingsErrorMessage,
} from "./teams";
import type { Member, ProcessPage, Role, TeamProcess } from "./types";

const member = (id: string, role: Role): Member => ({
  user: { id, login: id, name: id, avatarUrl: "" },
  role,
  joinedAt: "2026-10-01T00:00:00.000Z",
});

const process = (id: string, seq: number, deleted = false): TeamProcess => ({
  id,
  teamId: "t",
  name: id,
  version: 1,
  seq,
  deleted,
  updatedBy: "u",
  updatedAt: "2026-10-01T00:00:00.000Z",
  definition: {},
});

describe("isValidTeamName", () => {
  it("takes 1 to 80 characters after trimming", () => {
    expect(isValidTeamName("Northwind")).toBe(true);
    expect(isValidTeamName("   ")).toBe(false);
    expect(isValidTeamName("x".repeat(80))).toBe(true);
    expect(isValidTeamName(`  ${"x".repeat(80)}  `)).toBe(true);
    expect(isValidTeamName("x".repeat(81))).toBe(false);
  });
});

describe("roles", () => {
  it("only offers Owner to owners", () => {
    expect(roleChoices("owner")).toEqual(["owner", "admin", "member"]);
    expect(roleChoices("admin")).toEqual(["admin", "member"]);
  });

  it("lets admins change everyone but owners", () => {
    expect(canChangeRole("owner", member("a", "owner"))).toBe(true);
    expect(canChangeRole("admin", member("a", "owner"))).toBe(false);
    expect(canChangeRole("admin", member("a", "member"))).toBe(true);
    expect(canChangeRole("member", member("a", "member"))).toBe(false);
  });

  it("never offers × on your own row", () => {
    expect(canRemove("owner", "me", member("me", "owner"))).toBe(false);
    expect(canRemove("owner", "me", member("you", "admin"))).toBe(true);
    expect(canRemove("admin", "me", member("you", "owner"))).toBe(false);
    expect(canRemove("member", "me", member("you", "member"))).toBe(false);
  });
});

describe("error copy", () => {
  it("words each join failure", () => {
    expect(joinErrorMessage(410)).toBe(
      "This invite has expired or was already used. Ask whoever sent it for a new one.",
    );
    expect(joinErrorMessage(404)).toBe("That code isn't right. Check it and try again.");
    expect(joinErrorMessage(409)).toBe("You're already in this team.");
    expect(joinErrorMessage(0)).toMatch(/offline/);
    expect(joinErrorMessage(503)).toMatch(/offline/);
  });

  it("words the last owner rule", () => {
    expect(settingsErrorMessage(409)).toBe("A team needs at least one owner. Make someone else an owner first.");
  });
});

describe("joined copy", () => {
  it("matches the design with every count", () => {
    expect(joinedSummary({ members: 6, processes: 4, connections: 3, role: "member" })).toBe(
      "6 members share its 4 processes and 3 connections. You joined as a member.",
    );
  });

  it("drops connections when they aren't known", () => {
    expect(joinedSummary({ members: 6, processes: 4, connections: null, role: "admin" })).toBe(
      "6 members share its 4 processes. You joined as an admin.",
    );
  });

  it("reads naturally with nothing shared or nothing loaded", () => {
    expect(joinedSummary({ members: 2, processes: 0, connections: null, role: "member" })).toBe(
      "It has 2 members. You joined as a member.",
    );
    expect(joinedSummary({ members: null, processes: 4, connections: null, role: "member" })).toBe(
      "You joined as a member.",
    );
    expect(joinedSummary({ members: 2, processes: 1, connections: null, role: "member" })).toBe(
      "2 members share its 1 process. You joined as a member.",
    );
  });

  it("counts personal processes", () => {
    expect(personalSummary(3)).toBe("Stays on this computer. Your 3 processes are not shared.");
    expect(personalSummary(1)).toBe("Stays on this computer. Your 1 process is not shared.");
    expect(personalSummary(undefined)).toBe("Stays on this computer. Your processes are not shared.");
  });
});

describe("countLiveProcesses", () => {
  it("pages through the feed and skips tombstones", async () => {
    const pages: Record<number, ProcessPage> = {
      0: { processes: [process("a", 1), process("b", 2)], lastSeq: 5, hasMore: true },
      2: { processes: [process("c", 3, true), process("a", 4)], lastSeq: 5, hasMore: true },
      4: { processes: [process("b", 5, true)], lastSeq: 5, hasMore: false },
    };
    const asked: number[] = [];
    const count = await countLiveProcesses(async (since) => {
      asked.push(since);
      return pages[since]!;
    });
    expect(asked).toEqual([0, 2, 4]);
    expect(count).toBe(1);
  });

  it("handles an empty team", async () => {
    expect(await countLiveProcesses(async () => ({ processes: [], lastSeq: 0, hasMore: false }))).toBe(0);
  });
});

describe("invites", () => {
  const now = new Date(2026, 9, 11, 12, 0);

  it("formats the expiry as day and month", () => {
    expect(formatShortDate(new Date(2026, 9, 18, 9, 30).toISOString(), now)).toBe("18 Oct");
    expect(formatShortDate(new Date(2027, 0, 3, 9, 30).toISOString(), now)).toBe("3 Jan 2027");
    expect(formatShortDate("not a date", now)).toBe("");
  });

  it("summarises the card", () => {
    const expiresAt = new Date(2026, 9, 18, 9, 30).toISOString();
    expect(inviteSummary({ role: "member", expiresAt }, now)).toBe("Works once · joins as Member · expires 18 Oct");
    expect(inviteSummary({ role: "admin", expiresAt }, now)).toBe("Works once · joins as Admin · expires 18 Oct");
  });

  it("shows the link without its scheme", () => {
    expect(displayInviteUrl("https://augry.app/join/K7M2P-9XQ4R")).toBe("augry.app/join/K7M2P-9XQ4R");
  });
});
