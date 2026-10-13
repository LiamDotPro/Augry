import { describe, expect, it } from "vitest";
import type { Team } from "../../cloud/types";
import {
  clockTime,
  codeMinutes,
  deviceCount,
  deviceUsage,
  listNames,
  notesSyncedAt,
  notesSyncNames,
  processCount,
  shortDate,
  spaceDetail,
} from "./accountText";

// Local times, so the tests pass in any time zone.
const at = (y: number, m: number, d: number, h = 12, min = 0) => new Date(y, m - 1, d, h, min);
const iso = (date: Date) => date.toISOString();
const now = at(2026, 10, 12, 15, 30);

const team = (id: string, name: string): Team => ({
  id,
  name,
  slug: name.toLowerCase(),
  plan: "free",
  role: "owner",
  createdAt: "2026-10-01T00:00:00.000Z",
});

describe("shortDate", () => {
  it("drops the year when it is this year", () => {
    expect(shortDate(at(2026, 10, 11), now)).toBe("11 Oct");
    expect(shortDate(at(2025, 3, 2), now)).toBe("2 Mar 2025");
  });
});

describe("deviceUsage", () => {
  it("says used today for this device", () => {
    expect(deviceUsage(iso(at(2026, 10, 11)), iso(at(2026, 10, 12, 0, 5)), now)).toBe("Signed in 11 Oct · used today");
  });

  it("says yesterday, then the date", () => {
    expect(deviceUsage(iso(at(2026, 10, 1)), iso(at(2026, 10, 11, 23, 59)), now)).toBe(
      "Signed in 1 Oct · used yesterday",
    );
    expect(deviceUsage(iso(at(2026, 10, 1)), iso(at(2026, 10, 10)), now)).toBe("Signed in 1 Oct · last used 10 Oct");
  });
});

describe("deviceCount", () => {
  it("counts devices", () => {
    expect(deviceCount(1)).toBe("Signed in on 1 device");
    expect(deviceCount(2)).toBe("Signed in on 2 devices");
  });
});

describe("clockTime", () => {
  it("pads hours and minutes", () => {
    expect(clockTime(at(2026, 10, 12, 9, 5).getTime())).toBe("09:05");
    expect(clockTime(at(2026, 10, 12, 10, 42).getTime())).toBe("10:42");
  });
});

describe("listNames", () => {
  it("joins with commas and a final and", () => {
    expect(listNames([])).toBe("");
    expect(listNames(["Personal"])).toBe("Personal");
    expect(listNames(["Personal", "Northwind"])).toBe("Personal and Northwind");
    expect(listNames(["Personal", "A", "B"])).toBe("Personal, A and B");
  });
});

describe("notes sync", () => {
  const teams = [team("t1", "Northwind"), team("t2", "Acme")];

  it("is off until chosen, or when nothing is on", () => {
    expect(notesSyncNames(null, teams)).toBeNull();
    expect(notesSyncNames({ personal: false, teams: [] }, teams)).toBeNull();
    expect(notesSyncedAt(null, teams, { "notes:personal": 1 })).toBeNull();
  });

  it("names what syncs and takes the latest sync time", () => {
    const synced = { "notes:personal": at(2026, 10, 12, 10, 40).getTime(), "notes:t1": at(2026, 10, 12, 10, 42).getTime() };
    const choice = { personal: true, teams: ["t1"] };
    expect(notesSyncNames(choice, teams)).toBe("Personal and Northwind");
    expect(clockTime(notesSyncedAt(choice, teams, synced)!)).toBe("10:42");
  });

  it("has no time before the first sync and skips teams you left", () => {
    const choice = { personal: true, teams: ["gone", "t2"] };
    expect(notesSyncNames(choice, teams)).toBe("Personal and Acme");
    expect(notesSyncedAt(choice, teams, {})).toBeNull();
  });
});

describe("spaceDetail", () => {
  it("adds the process count when known", () => {
    expect(spaceDetail("personal", 3)).toBe("On this computer · 3 processes");
    expect(spaceDetail("personal", undefined)).toBe("On this computer");
    expect(spaceDetail({ role: "owner" }, 4)).toBe("Team · owner · 4 processes");
    expect(spaceDetail({ role: "member" }, 1)).toBe("Team · member · 1 process");
    expect(processCount(0)).toBe("0 processes");
  });
});

describe("codeMinutes", () => {
  it("rounds to whole minutes, at least one", () => {
    expect(codeMinutes(900_000 + 1000, 1000)).toBe(15);
    expect(codeMinutes(1000, 1000)).toBe(1);
  });
});
