import { describe, expect, it } from "vitest";
import { indentOf, languageOf, lineEndingOf, reanchor, reviewMessage, reviewMessageOneLine } from "./review";

const comments = [
  { path: "src/ipc/types.ts", line: 12, text: "Make pageIndex optional so old sessions still load." },
  { path: "src/components/PaneGrid.tsx", line: 28, text: "Keep global pane numbers\nso hotkeys match" },
  { path: "src/components/PaneGrid.tsx", line: 24, text: "Stop at the last page instead of wrapping" },
];

describe("reviewMessage", () => {
  it("groups comments by file in line order and ends with the note", () => {
    expect(reviewMessage(comments, "  Also add a test for the last-page case. ")).toBe(
      [
        "Review comments from Augry (3). Please address them:",
        "",
        "src/components/PaneGrid.tsx:24",
        "  Stop at the last page instead of wrapping",
        "",
        "src/components/PaneGrid.tsx:28",
        "  Keep global pane numbers",
        "  so hotkeys match",
        "",
        "src/ipc/types.ts:12",
        "  Make pageIndex optional so old sessions still load.",
        "",
        "Also add a test for the last-page case.",
      ].join("\n"),
    );
  });

  it("has a one line form without newlines", () => {
    const text = reviewMessageOneLine(comments, "note\nhere");
    expect(text).not.toContain("\n");
    expect(text).toContain("src/components/PaneGrid.tsx:28: Keep global pane numbers so hotkeys match;");
    expect(text).toContain("src/ipc/types.ts:12: Make pageIndex optional so old sessions still load.");
    expect(text).not.toContain(".;");
    expect(text.endsWith("Note: note here")).toBe(true);
  });
});

describe("reanchor", () => {
  const lines = ["a", "b", "target", "c"];

  it("keeps a line that still holds its text", () => {
    expect(reanchor(lines, 3, "target")).toBe(3);
  });

  it("follows the text when lines were inserted above", () => {
    expect(reanchor(["x", "y", ...lines], 3, "target")).toBe(5);
  });

  it("falls back to the line number, clamped to the file", () => {
    expect(reanchor(lines, 9, "gone")).toBe(4);
  });
});

describe("file facts", () => {
  it("names the language from the extension", () => {
    expect(languageOf("src/a.tsx")).toBe("TypeScript");
    expect(languageOf("Makefile")).toBe("Plain text");
  });

  it("detects line endings and indentation", () => {
    expect(lineEndingOf("a\r\nb\r\n")).toBe("CRLF");
    expect(lineEndingOf("a\nb\n")).toBe("LF");
    expect(indentOf("a\n  b\n    c\n").label).toBe("2 spaces");
    expect(indentOf("a\n    b\n        c\n").label).toBe("4 spaces");
    expect(indentOf("a\n\tb\n").label).toBe("Tabs");
  });
});
