import { describe, expect, it, vi } from "vitest";
import type { TeamConnection } from "../cloud/types";
import { CONNECTIONS } from "./connections";
import { canEdit, connectionName, loadTeamConnections, referenceOf, teamConnectionSections } from "./teamConnections";

vi.mock("../cloud/api", () => ({
  listTeamConnections: vi.fn(async () => {
    throw new Error("should not be called");
  }),
}));

const maya = { id: "u-maya", login: "maya", name: "Maya Chen", avatarUrl: "" };
const me = { id: "u-me", login: "liam", name: "Liam", avatarUrl: "" };

function conn(kind: string, settings: Record<string, string>, by = maya, extra: Partial<TeamConnection> = {}): TeamConnection {
  return {
    id: `c-${kind}`, teamId: "t1", name: kind, kind, settings, hasSecret: true, editors: "admins", version: 1, seq: 1, deleted: false,
    createdBy: by, updatedBy: by, updatedAt: "2026-10-10T10:00:00.000Z", ...extra,
  };
}

const addedBy = (u: { id: string; name: string }) => (u.id === me.id ? "you" : u.name);

describe("teamConnectionSections", () => {
  it("shows the team's connections, unset kinds and local ones as the design does", () => {
    const team = [
      conn("github", { account: "northwind-bot" }),
      conn("postgresql", { host: "staging-db.northwind.dev:5432", name: "Staging database" }),
      conn("redis", { host: "cache.northwind.dev:6379" }, me),
      conn("linear", { account: "gone" }, maya, { deleted: true }),
    ];
    const [work, systems, local] = teamConnectionSections(CONNECTIONS, team, "Northwind", addedBy);
    expect(work!.hint).toBe("Shared with Northwind · one sign-in for the whole team");
    expect(work!.cards.map((c) => c.status)).toEqual(["Shared · northwind-bot", "Not set up for the team", "Not set up for the team", "Not set up for the team"]);
    expect(work!.cards[0]!.used).toBe("Added by Maya Chen");
    expect(systems!.cards.map((c) => c.status)).toEqual([
      "Shared · staging-db.northwind.dev",
      "Not set up for the team",
      "Not set up for the team",
      "Shared · cache.northwind.dev",
    ]);
    expect(systems!.cards[3]!.used).toBe("Added by you");
    expect(local!.title).toBe("On this computer");
    expect(local!.hint).toBe("Not shared. Each member sets these up on their own computer");
    expect(local!.cards.map((c) => c.used)).toEqual(["Yours only", "Yours only", "Yours only", "Yours only"]);
  });

  it("reads every shared kind as not set up without the feature", () => {
    const sections = teamConnectionSections(CONNECTIONS, null, "Northwind", addedBy);
    expect(sections[0]!.cards.every((c) => !c.on && c.status === "Not set up for the team")).toBe(true);
    expect(sections[2]!.cards.every((c) => c.mine)).toBe(true);
  });
});

describe("rules", () => {
  it("makes names the API accepts", () => {
    expect(connectionName("Staging database", [])).toBe("staging-database");
    expect(connectionName("postgresql", [conn("postgresql", {})])).toBe("postgresql-2");
    expect(referenceOf("systems", "staging")).toBe("db.staging");
    expect(referenceOf("work", "github")).toBe("github");
  });

  it("lets members change only connections open to everyone", () => {
    expect(canEdit(conn("github", {}), "member")).toBe(false);
    expect(canEdit(conn("github", {}, maya, { editors: "members" }), "member")).toBe(true);
    expect(canEdit(null, "admin")).toBe(true);
  });

  it("never calls the API without the feature", async () => {
    expect(await loadTeamConnections("t1", false)).toBeNull();
  });
});
