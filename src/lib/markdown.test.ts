import { describe, expect, it } from "vitest";
import { classifyLines, parseInline, parseMarkdown } from "./markdown";

describe("parseMarkdown", () => {
  it("reads the note the design shows in the preview", () => {
    const blocks = parseMarkdown(
      [
        "# Claude Code — working notes",
        "",
        "Runs in pane 1 and 6 on `augry`. Keep one agent per",
        "worktree so diffs stay isolated.",
        "",
        "## Prompts that worked",
        "",
        '- Ask for a plan first, then *"implement step 1 only"*.',
        "- Point it at `src/ipc/types.ts` before any UI change.",
        "",
      ].join("\n"),
    );

    expect(blocks.map((block) => block.type)).toEqual([
      "heading",
      "paragraph",
      "heading",
      "list",
    ]);
    expect(blocks[0]).toMatchObject({ level: 1 });
    expect(blocks[3]).toMatchObject({ ordered: false, items: expect.any(Array) });
    if (blocks[3].type === "list") expect(blocks[3].items).toHaveLength(2);
  });

  it("joins a soft wrapped paragraph into one block", () => {
    const blocks = parseMarkdown("one\ntwo\n\nthree");
    expect(blocks).toHaveLength(2);
    expect(blocks[0]).toMatchObject({ type: "paragraph" });
    if (blocks[0].type === "paragraph") {
      expect(blocks[0].children).toEqual([{ type: "text", value: "one two" }]);
    }
  });

  it("keeps fenced code verbatim, including markdown inside it", () => {
    const blocks = parseMarkdown("```ts\nconst x = `# not a heading`;\n```");
    expect(blocks[0]).toEqual({
      type: "code",
      lang: "ts",
      value: "const x = `# not a heading`;",
    });
  });

  it("reads ordered lists and their start", () => {
    const blocks = parseMarkdown("3. three\n4. four");
    expect(blocks[0]).toMatchObject({ type: "list", ordered: true, start: 3 });
  });

  it("reads task list items and drops their markers", () => {
    const blocks = parseMarkdown("- [x] done\n- [ ] todo\n- plain\n- [X] `code` too");
    expect(blocks[0]).toMatchObject({ type: "list", tasks: [true, false, null, true] });
    if (blocks[0].type === "list") {
      expect(blocks[0].items[0]).toEqual([{ type: "paragraph", children: [{ type: "text", value: "done" }] }]);
      expect(blocks[0].items[3]).toEqual([
        { type: "paragraph", children: [{ type: "code", value: "code" }, { type: "text", value: " too" }] },
      ]);
    }
  });

  it("leaves brackets that are not a task marker alone", () => {
    const blocks = parseMarkdown("- [link](https://example.com)\n- [y] not a task");
    expect(blocks[0]).toMatchObject({ tasks: [null, null] });
  });

  it("reads block quotes and rules", () => {
    expect(parseMarkdown("> quoted")[0]).toMatchObject({ type: "quote" });
    expect(parseMarkdown("---")[0]).toEqual({ type: "rule" });
  });
});

describe("parseInline", () => {
  it("reads code, emphasis and links", () => {
    expect(parseInline("a `b` c")).toEqual([
      { type: "text", value: "a " },
      { type: "code", value: "b" },
      { type: "text", value: " c" },
    ]);
    expect(parseInline("**bold**")).toEqual([
      { type: "strong", children: [{ type: "text", value: "bold" }] },
    ]);
    expect(parseInline("[docs](https://example.com)")).toEqual([
      { type: "link", href: "https://example.com", children: [{ type: "text", value: "docs" }] },
    ]);
  });

  it("neutralises a script url but keeps a relative one", () => {
    expect(parseInline("[x](javascript:alert)")).toEqual([
      { type: "link", href: "", children: [{ type: "text", value: "x" }] },
    ]);
    expect(parseInline("[x](vbscript:msgbox)")).toEqual([
      { type: "link", href: "", children: [{ type: "text", value: "x" }] },
    ]);
    expect(parseInline("[x](../other.md)")).toEqual([
      { type: "link", href: "../other.md", children: [{ type: "text", value: "x" }] },
    ]);
  });

  it("leaves a destination it cannot parse as literal text", () => {
    expect(parseInline("[x](javascript:alert(1))")).toEqual([
      { type: "text", value: "[x](javascript:alert(1))" },
    ]);
  });

  it("leaves plain text alone", () => {
    expect(parseInline("nothing special")).toEqual([
      { type: "text", value: "nothing special" },
    ]);
  });
});

describe("classifyLines", () => {
  it("labels each line for the editor gutter", () => {
    expect(classifyLines("# h\ntext\n- item\n> quote")).toEqual([
      "heading",
      "text",
      "list",
      "quote",
    ]);
  });

  it("treats everything between fences as code", () => {
    expect(classifyLines("```\n# not a heading\n```\n# heading")).toEqual([
      "code",
      "code",
      "code",
      "heading",
    ]);
  });

  it("returns one entry per line, including the trailing empty one", () => {
    expect(classifyLines("a\n")).toHaveLength(2);
  });
});
