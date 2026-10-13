import { describe, expect, it } from "vitest";
import {
  conflictCopyPath,
  isSyncableNote,
  joinRoot,
  kindOf,
  localOfSpace,
  MAX_PATH_BYTES,
  pathProblem,
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

describe("isSyncableNote", () => {
  it("takes .md only, case sensitive like the server", () => {
    expect(isSyncableNote("plan.md")).toBe(true);
    expect(isSyncableNote("plan.MD")).toBe(false);
    expect(isSyncableNote("plan.Md")).toBe(false);
    expect(spaceOfLocal("Northwind/plan.MD", layout)).toBeNull();
  });
});

describe("pathProblem", () => {
  it("takes ordinary paths", () => {
    expect(pathProblem("team", "agents/claude-code.md")).toBeNull();
    expect(pathProblem("team", "é/日本語 notes.md")).toBeNull();
    expect(pathProblem("personal", "settings.json")).toBeNull();
    expect(pathProblem("team", "..notes.md")).toBeNull();
  });

  it("refuses an empty path", () => {
    expect(pathProblem("team", "")).toBe("path is empty");
  });

  it("counts the limit in UTF-8 bytes, not characters", () => {
    const fits = `${"a".repeat(MAX_PATH_BYTES - 3)}.md`;
    expect(pathProblem("team", fits)).toBeNull();
    expect(pathProblem("team", `a${fits}`)).toBe("path is longer than 512 bytes");
    // 170 three-byte characters are 510 bytes: with ".md", 513.
    expect(pathProblem("team", `${"日".repeat(170)}.md`)).toBe("path is longer than 512 bytes");
    expect(pathProblem("team", `${"日".repeat(169)}.md`)).toBeNull();
  });

  it("refuses half a surrogate pair as not UTF-8", () => {
    expect(pathProblem("team", "a\ud800.md")).toBe("path is not valid UTF-8");
    expect(pathProblem("team", "a\udc00.md")).toBe("path is not valid UTF-8");
    expect(pathProblem("team", "a😀.md")).toBeNull();
  });

  it("refuses control characters: C0, DEL and C1", () => {
    for (const control of ["\u0000", "\t", "\n", "\u001f", "\u007f", "\u0080", "\u009f"]) {
      expect(pathProblem("team", `a${control}b.md`)).toBe("path has a control character");
    }
    expect(pathProblem("team", "a b.md")).toBeNull();
  });

  it("refuses a backslash", () => {
    expect(pathProblem("team", "a\\b.md")).toBe("path has a backslash");
  });

  it("refuses a leading or trailing slash", () => {
    expect(pathProblem("team", "/a.md")).toBe("path starts with a slash");
    expect(pathProblem("team", "a.md/")).toBe("path ends with a slash");
  });

  it("refuses empty, . and .. segments", () => {
    expect(pathProblem("team", "a//b.md")).toBe("path has an empty segment");
    expect(pathProblem("team", "./a.md")).toBe("path has a . or .. segment");
    expect(pathProblem("team", "a/../b.md")).toBe("path has a . or .. segment");
  });

  it("takes .md in a team space and .md or .json in personal, case sensitive", () => {
    expect(pathProblem("team", "a.MD")).toBe("path must end in .md");
    expect(pathProblem("team", "settings.json")).toBe("path must end in .md");
    expect(pathProblem("personal", "a.txt")).toBe("path must end in .md or .json");
    expect(pathProblem("personal", "a.JSON")).toBe("path must end in .md or .json");
    expect(pathProblem("personal", "a.md")).toBeNull();
  });

  it("checks the rules in the server's order", () => {
    expect(pathProblem("team", "/a\\b")).toBe("path has a backslash");
    expect(pathProblem("team", "/a//b")).toBe("path starts with a slash");
    expect(pathProblem("team", "a/./b")).toBe("path has a . or .. segment");
  });

  it("names the kind of a space", () => {
    expect(kindOf("personal")).toBe("personal");
    expect(kindOf("t1")).toBe("team");
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
