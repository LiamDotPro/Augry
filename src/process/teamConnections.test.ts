import { describe, expect, it, vi } from "vitest";
import { CloudError } from "../cloud/api";
import type { TeamConnection } from "../cloud/types";
import { CONNECTIONS } from "./connections";
import {
  canChangeEditors,
  canCreate,
  canEdit,
  connectionErrorMessage,
  connectionName,
  connectionRef,
  isValidConnectionName,
  loadTeamConnections,
  nameForRef,
  NAME_RULE,
  saveRequest,
  secretProblem,
  settingsJsonBytes,
  settingsProblem,
  sharedSettings,
  shareRowOf,
  teamConnectionForRef,
  teamConnectionSections,
} from "./teamConnections";

vi.mock("../cloud/api", async (original) => ({
  ...(await original<typeof import("../cloud/api")>()),
  listTeamConnections: vi.fn(async () => {
    throw new Error("should not be called");
  }),
}));

const maya = { id: "u-maya", login: "maya", name: "Maya Chen", avatarUrl: "" };
const me = { id: "u-me", login: "liam", name: "Liam", avatarUrl: "" };

function conn(name: string, kind: string, settings: Record<string, string>, by = maya, extra: Partial<TeamConnection> = {}): TeamConnection {
  return {
    id: `c-${name}`, teamId: "t1", name, kind, settings, hasSecret: true, editors: "admins", version: 1, seq: 1, deleted: false,
    createdBy: by, createdAt: "2026-10-10T09:00:00.000Z", updatedBy: by, updatedAt: "2026-10-10T10:00:00.000Z", ...extra,
  };
}

const addedBy = (u: { id: string; name: string }) => (u.id === me.id ? "you" : u.name);
const local = (icon: string) => CONNECTIONS.find((c) => c.icon === icon)!;

describe("teamConnectionSections", () => {
  it("gives every live team connection its own card, several of a kind included", () => {
    const team = [
      conn("github", "github", { account: "northwind-bot" }),
      conn("github-ops", "github", { account: "northwind-ops" }, me),
      conn("staging", "postgresql", { host: "staging-db.northwind.dev:5432" }),
      conn("cache", "redis", { host: "cache.northwind.dev:6379" }, me),
      conn("old-linear", "linear", { account: "gone" }, maya, { deleted: true }),
    ];
    const [work, systems, onThisComputer] = teamConnectionSections(CONNECTIONS, team, "Northwind", addedBy);
    expect(work!.hint).toBe("Shared with Northwind · one sign-in for the whole team");
    expect(work!.cards.map((c) => [c.title, c.status])).toEqual([
      ["GitHub Issues · github", "Shared · northwind-bot"],
      ["GitHub Issues · github-ops", "Shared · northwind-ops"],
      ["Linear", "Not set up for the team"],
      ["Jira", "Not set up for the team"],
      ["GitLab", "Not set up for the team"],
    ]);
    expect(work!.cards.map((c) => c.key)).toEqual(["c-github", "c-github-ops", "linear", "jira", "gitlab"]);
    expect(work!.cards[1]!.used).toBe("Added by you");
    expect(systems!.cards.map((c) => c.status)).toEqual([
      "Shared · staging-db.northwind.dev",
      "Not set up for the team",
      "Not set up for the team",
      "Shared · cache.northwind.dev",
    ]);
    expect(onThisComputer!.title).toBe("On this computer");
    expect(onThisComputer!.cards.map((c) => c.used)).toEqual(["Yours only", "Yours only", "Yours only", "Yours only"]);
  });

  it("puts kinds this app doesn't know last in Systems and data", () => {
    const [, systems] = teamConnectionSections(CONNECTIONS, [conn("vault", "hashicorp-vault", {})], "Northwind", addedBy);
    const last = systems!.cards[systems!.cards.length - 1]!;
    expect(last.title).toBe("hashicorp-vault · vault");
    expect(last.team?.name).toBe("vault");
  });

  it("reads every shared kind as not set up without the feature", () => {
    const sections = teamConnectionSections(CONNECTIONS, null, "Northwind", addedBy);
    expect(sections[0]!.cards.every((c) => !c.on && c.status === "Not set up for the team")).toBe(true);
    expect(sections[2]!.cards.every((c) => c.mine)).toBe(true);
  });
});

describe("names", () => {
  it("follows the server's rule", () => {
    expect(isValidConnectionName("github-main")).toBe(true);
    expect(isValidConnectionName("1password")).toBe(true);
    expect(isValidConnectionName("a".repeat(64))).toBe(true);
    for (const bad of ["", "-a", "GitHub", "a_b", "a.b", "a b", "a".repeat(65), "café"]) expect(isValidConnectionName(bad)).toBe(false);
  });

  it("makes free names the API accepts", () => {
    expect(connectionName("Staging database", [])).toBe("staging-database");
    expect(connectionName("postgresql", [conn("postgresql", "postgresql", {})])).toBe("postgresql-2");
    expect(connectionName("postgresql", [conn("postgresql", "postgresql", {}, maya, { deleted: true })])).toBe("postgresql");
    expect(connectionRef("staging")).toBe("${conn:staging}");
  });

  it("finds the team connection a reference names, not one of the same kind", () => {
    const team = [conn("github-main", "github", {}), conn("staging", "postgresql", {}), conn("old", "github", {}, maya, { deleted: true })];
    expect(teamConnectionForRef("github", team)).toBeNull();
    expect(teamConnectionForRef("github-main", team)?.id).toBe("c-github-main");
    expect(teamConnectionForRef("db.staging", team)?.id).toBe("c-staging");
    expect(teamConnectionForRef("old", team)).toBeNull();
    expect(nameForRef("db.staging")).toBe("staging");
    expect(nameForRef("My_DB")).toBeNull();
  });
});

describe("who may do what", () => {
  it("lets only owners and admins add connections and choose who can change them", () => {
    expect(canCreate("owner")).toBe(true);
    expect(canCreate("admin")).toBe(true);
    expect(canCreate("member")).toBe(false);
    expect(canChangeEditors("admin")).toBe(true);
    expect(canChangeEditors("member")).toBe(false);
  });

  it("lets members change and remove only connections open to everyone", () => {
    expect(canEdit(conn("github", "github", {}), "member")).toBe(false);
    expect(canEdit(conn("github", "github", {}, maya, { editors: "members" }), "member")).toBe(true);
    expect(canEdit(conn("github", "github", {}), "admin")).toBe(true);
  });
});

describe("saveRequest", () => {
  const stored = conn("github", "github", { account: "a" }, maya, { version: 3 });

  it("keeps the secret when it is left out and removes it with an empty one", () => {
    expect(saveRequest(stored, { settings: { account: "b" }, editors: "admins" })).toEqual({ settings: { account: "b" }, baseVersion: 3 });
    expect(saveRequest(stored, { settings: {}, secret: "", editors: "admins" })).toEqual({ settings: {}, secret: "", baseVersion: 3 });
    expect(saveRequest(stored, { settings: {}, secret: "tok", editors: "admins" }).secret).toBe("tok");
  });

  it("sends editors only when it changed", () => {
    expect(saveRequest(stored, { settings: {}, editors: "admins" })).not.toHaveProperty("editors");
    expect(saveRequest(stored, { settings: {}, editors: "members" }).editors).toBe("members");
  });
});

describe("limits", () => {
  it("checks settings as the server does", () => {
    expect(settingsProblem({ host: "db.example.com" })).toBeNull();
    const many = Object.fromEntries(Array.from({ length: 51 }, (_, i) => [`k${i}`, "v"]));
    expect(settingsProblem(many)).toBe("That's 51 settings. A connection can have at most 50.");
    expect(settingsProblem({ "a b": "v" })).toMatch(/isn't one the server takes/);
    expect(settingsProblem({ host: "é".repeat(1001) }, (k) => (k === "host" ? "Host and port" : k))).toBe("Host and port is longer than 2,000 bytes. Shorten it.");
    const big = Object.fromEntries(Array.from({ length: 9 }, (_, i) => [`k${i}`, "y".repeat(1900)]));
    expect(settingsProblem(big)).toBe("The settings are larger than 16 KB together. Shorten them.");
  });

  it("measures the JSON as Go writes it", () => {
    expect(settingsJsonBytes({ b: "1", a: "2" })).toBe('{"a":"2","b":"1"}'.length);
    expect(settingsJsonBytes({ a: "<&>" })).toBe('{"a":"\\u003c\\u0026\\u003e"}'.length);
  });

  it("refuses a token in the settings and says where it belongs", () => {
    expect(settingsProblem({ account: `ghp_${"a".repeat(36)}` }, () => "Account or workspace", "the Token field")).toBe(
      "Account or workspace looks like a GitHub token. Every member can read settings, so put it in the Token field instead, where it is encrypted.",
    );
  });

  it("caps the secret at 8192 bytes", () => {
    expect(secretProblem("x".repeat(8192))).toBeNull();
    expect(secretProblem("x".repeat(8193))).toMatch(/longer than 8,192 bytes/);
  });
});

describe("the share dialog", () => {
  const github = { ref: "github", label: "GitHub Issues", local: local("github"), kind: "github" };

  it("says the team has it only when a connection has the name the file uses", () => {
    expect(shareRowOf(github, null, "admin", "Northwind")).toEqual({ state: "unknown" });
    expect(shareRowOf(github, [conn("github", "github", {})], "admin", "Northwind").state).toBe("has");
    expect(shareRowOf(github, [conn("github-main", "github", {})], "admin", "Northwind")).toEqual({ state: "missing", name: "github", canShare: true, why: null });
  });

  it("explains why a member or an unusable name can't share", () => {
    const member = shareRowOf(github, [], "member", "Northwind");
    expect(member).toMatchObject({ state: "missing", canShare: false });
    expect(member.state === "missing" && member.why).toBe("Only owners and admins can add connections to Northwind. Ask one to add github in Connections.");
    const odd = shareRowOf({ ...github, ref: "GitHub_Main" }, [], "owner", "Northwind");
    expect(odd.state === "missing" && odd.why).toContain(NAME_RULE);
    expect(shareRowOf({ ref: "jira", label: "jira" }, [], "admin", "Northwind")).toEqual({ state: "missing", name: "jira", canShare: false, why: null });
  });

  it("never copies a field that holds a secret", () => {
    expect(sharedSettings({ ...local("postgresql"), settings: { host: "h", password: "p", user: "u" } })).toEqual({ host: "h", user: "u" });
  });
});

describe("connectionErrorMessage", () => {
  const ctx = { teamName: "Northwind", name: "github" };

  it("words a create's 409 as a name clash and a save's as a newer version", () => {
    expect(connectionErrorMessage(new CloudError(409, "conflict", "a connection named github already exists"), { ...ctx, action: "create" })).toBe(
      "Northwind already has a connection named github. Pick another name.",
    );
    const current = conn("github", "github", {}, maya, { version: 2 });
    expect(connectionErrorMessage(new CloudError(409, "conflict", "the connection changed since baseVersion", current), { ...ctx, action: "save" })).toBe(
      "Someone changed it a moment ago. Look it over and save again.",
    );
    expect(
      connectionErrorMessage(new CloudError(409, "conflict", "the connection is deleted", { ...current, deleted: true }), { ...ctx, action: "remove" }),
    ).toBe("Someone removed it a moment ago.");
  });

  it("words 403s by what was refused", () => {
    expect(connectionErrorMessage(new CloudError(403, "forbidden", "this needs the admin role"), { ...ctx, action: "create" })).toBe(
      "Only owners and admins can add connections to Northwind.",
    );
    expect(
      connectionErrorMessage(new CloudError(403, "forbidden", "changing who can edit a connection needs the admin role"), { ...ctx, action: "save" }),
    ).toBe("Only owners and admins can change who can change it.");
  });

  it("words the server's limits and secret scan", () => {
    expect(connectionErrorMessage(new CloudError(400, "bad_request", "this team already has 100 connections"), { ...ctx, action: "create" })).toBe(
      "Northwind already has 100 connections, the most a team can have. Remove one first.",
    );
    expect(connectionErrorMessage(new CloudError(400, "bad_request", "settings has 51 keys, the limit is 50"), { ...ctx, action: "save" })).toBe(
      "A connection can have at most 50 settings.",
    );
    expect(
      connectionErrorMessage(new CloudError(422, "unprocessable", "settings_contains_secret: github-token at $.account"), {
        ...ctx,
        action: "save",
        labelOf: (k) => (k === "account" ? "Account or workspace" : k),
        secretLabel: "the Token field",
      }),
    ).toBe("Account or workspace looks like a GitHub token. Every member can read settings, so put it in the Token field instead, where it is encrypted.");
    expect(
      connectionErrorMessage(new CloudError(422, "unprocessable", 'settings_contains_secret: aws-access-key-id at $["a.b"]'), { ...ctx, action: "create" }),
    ).toMatch(/^a\.b looks like an AWS access key/);
  });

  it("tells a server without team connections from being offline", () => {
    expect(
      connectionErrorMessage(new CloudError(503, "unavailable", "team connections are not configured on this server"), { ...ctx, action: "create" }),
    ).toBe("Team connections aren't switched on for this server yet, so nothing was added.");
    expect(connectionErrorMessage(new CloudError(0, "offline", "offline"), { ...ctx, action: "save" })).toBe("You're offline. Try again once you're back online.");
    expect(connectionErrorMessage(new CloudError(502, "unknown", "HTTP 502"), { ...ctx, action: "load" })).toBe(
      "You're offline. Northwind's connections show once you're back online.",
    );
  });
});

describe("calls", () => {
  it("never calls the API without the feature", async () => {
    expect(await loadTeamConnections("t1", false)).toBeNull();
  });
});
