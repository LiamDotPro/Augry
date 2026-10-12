import { describe, expect, it } from "vitest";
import {
  conflictCopyPath,
  joinRoot,
  localOfSpace,
  separatorOf,
  spaceOfLocal,
  teamFolderName,
  type SpaceLayout,
} from "./paths";

const layout: SpaceLayout = {
  spaces: [
    { key: "personal", folder: "" },
    { key: "t1", folder: "Northwind" },
  ],
  teamFolders: ["Northwind", "Acme"],
};

describe("teamFolderName", () => {
  it("keeps ordinary names", () => {
    expect(teamFolderName("Northwind")).toBe("Northwind");
  });

  it("replaces characters a folder can't hold", () => {
    expect(teamFolderName("R&D / Ops: 2")).toBe("R&D - Ops- 2");
    expect(teamFolderName("..hidden.")).toBe("hidden");
    expect(teamFolderName("   ")).toBe("Team");
  });
});

describe("spaceOfLocal", () => {
  it("puts files in a synced team folder in that team's space", () => {
    expect(spaceOfLocal("Northwind/agents/claude-code.md", layout)).toEqual({
      space: "t1",
      path: "agents/claude-code.md",
    });
  });

  it("puts everything else in Personal", () => {
    expect(spaceOfLocal("repos/augry.md", layout)).toEqual({ space: "personal", path: "repos/augry.md" });
    expect(spaceOfLocal("Northwind.md", layout)).toEqual({ space: "personal", path: "Northwind.md" });
  });

  it("leaves out team folders that don't sync, and anything not markdown", () => {
    expect(spaceOfLocal("Acme/plan.md", layout)).toBeNull();
    expect(spaceOfLocal("Northwind/diagram.png", layout)).toBeNull();
    expect(spaceOfLocal("settings.json", layout)).toBeNull();
  });

  it("syncs nothing in Personal when personal sync is off", () => {
    const teamOnly = { ...layout, spaces: [{ key: "t1", folder: "Northwind" }] };
    expect(spaceOfLocal("todo.md", teamOnly)).toBeNull();
    expect(spaceOfLocal("Northwind/todo.md", teamOnly)).toEqual({ space: "t1", path: "todo.md" });
  });
});

describe("localOfSpace", () => {
  it("maps a team document into its folder", () => {
    expect(localOfSpace("t1", "agents/codex.md", layout)).toBe("Northwind/agents/codex.md");
  });

  it("refuses personal paths that would land in a team folder", () => {
    expect(localOfSpace("personal", "Acme/x.md", layout)).toBeNull();
    expect(localOfSpace("personal", "journal/x.md", layout)).toBe("journal/x.md");
  });

  it("leaves settings.json out", () => {
    expect(localOfSpace("personal", "settings.json", layout)).toBeNull();
  });
});

describe("conflictCopyPath", () => {
  it("puts their name in brackets next to yours", () => {
    expect(conflictCopyPath("agents/claude-code.md", "Maya Chen", () => false)).toBe(
      "agents/claude-code (Maya Chen).md",
    );
  });

  it("numbers the copy when the name is taken", () => {
    const taken = new Set(["todo (another device).md"]);
    expect(conflictCopyPath("todo.md", "another device", (path) => taken.has(path))).toBe(
      "todo (another device 2).md",
    );
  });
});

describe("joinRoot", () => {
  it("uses the notes folder's own separator", () => {
    expect(separatorOf("C:\\Users\\liam\\notes")).toBe("\\");
    expect(joinRoot("C:\\Users\\liam\\notes", "Northwind")).toBe("C:\\Users\\liam\\notes\\Northwind");
    expect(joinRoot("/Users/liam/notes/", "Northwind")).toBe("/Users/liam/notes/Northwind");
  });
});
