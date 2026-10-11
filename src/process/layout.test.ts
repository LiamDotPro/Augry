import { describe, expect, it } from "vitest";
import { BUG_FIX, CRASH_REPORT, FEATURE_DELIVERY, TEMPLATES } from "./examples";
import { buildGraph } from "./graph";
import { layoutLanes, miniBlocks, LANE_H, RULER_H } from "./layout";
import { runbookRows } from "./runbook";
import { cloneProcess } from "./json";

describe("buildGraph", () => {
  it("puts each step one column after the furthest step it waits on", () => {
    const { col, nCols } = buildGraph(FEATURE_DELIVERY);
    expect(col).toEqual({ plan: 0, approve: 1, impl: 2, checks: 3, passed: 4, smoke: 5, summary: 5, review: 6, pr: 7 });
    expect(nCols).toBe(8);
  });

  it("links decision branches and reads repeat loops off the decision", () => {
    const g = buildGraph(FEATURE_DELIVERY);
    expect(g.links.filter((l) => l.branch === "yes").map((l) => l.to)).toEqual(["smoke", "summary"]);
    expect(g.loops).toEqual([
      expect.objectContaining({ id: "passed", kind: "repeat", from: 2, to: 4, max: 3, label: "Repeat from Implement until “Checks pass”" }),
    ]);
  });

  it("finds for-each loops and the early end of a branch", () => {
    const g = buildGraph(BUG_FIX);
    expect(g.loops.map((l) => [l.id, l.from, l.to])).toEqual([
      ["green", 2, 4],
      ["each", 2, 4],
    ]);
    expect(g.preds.info).toEqual(["found"]);
  });

  it("survives references to missing steps and cycles from hand-edited JSON", () => {
    const p = cloneProcess(FEATURE_DELIVERY);
    p.steps[0]!.after = ["nope", "pr"];
    const g = buildGraph(p);
    expect(Object.values(g.col).every((c) => Number.isFinite(c) && c >= 0)).toBe(true);
  });
});

describe("layoutLanes", () => {
  it("draws decisions as diamonds on the decider's row in a narrow column", () => {
    const l = layoutLanes(FEATURE_DELIVERY, { colW: 150 });
    const passed = l.blocks.find((b) => b.id === "passed")!;
    expect(passed.kind).toBe("decision");
    expect(passed.glyph).toBe("=");
    const shellRow = FEATURE_DELIVERY.surfaces.findIndex((s) => s.id === "shell");
    expect(passed.y + 17).toBe(RULER_H + shellRow * LANE_H + LANE_H / 2);
    const impl = l.blocks.find((b) => b.id === "impl")!;
    expect(impl.w).toBe(130);
  });

  it("puts stages along the top, loops along the bottom and the start point top-left", () => {
    const l = layoutLanes(BUG_FIX, { colW: 196 });
    expect(l.stages.map((s) => s.name)).toEqual(["Triage", "Fix", "Ship"]);
    expect(l.stages[0]!.sep).toBe(false);
    expect(l.stages[1]!.sep).toBe(true);
    expect(l.loops).toHaveLength(2);
    expect(l.loops.every((b) => b.y >= l.lanesBottom)).toBe(true);
    expect(l.starts).toHaveLength(1);
    expect(l.starts[0]!.x).toBe(l.headW + 16);
    expect(l.ends.map((e) => e.name)).toEqual(["PR open", "Needs info"]);
    expect(l.labels.some((x) => x.text === "no ↺")).toBe(true);
  });

  it("marks the selected step while editing and shows max tries on a repeat loop", () => {
    const l = layoutLanes(FEATURE_DELIVERY, { selected: "passed" });
    expect(l.blocks.find((b) => b.id === "passed")!.state).toBe("selected");
    expect(l.loops[0]!.count).toBe("max 3");
    expect(l.play).toBeNull();
  });

  it("lays out every template without gaps", () => {
    for (const t of TEMPLATES) {
      const l = layoutLanes(t.process);
      expect(l.blocks).toHaveLength(t.process.steps.length);
      expect(l.width).toBeGreaterThan(l.headW);
    }
    expect(miniBlocks(CRASH_REPORT)).toHaveLength(9);
  });
});

describe("runbookRows", () => {
  it("reads the process stage by stage, with loops and parallel steps indented", () => {
    const rows = runbookRows(FEATURE_DELIVERY);
    expect(rows[0]).toMatchObject({ kind: "mark", role: "start", line: "GitHub issue · is:open label:ready" });
    expect(rows.filter((r) => r.kind === "stage").map((r) => (r.kind === "stage" ? r.name : ""))).toEqual(["Plan", "Build", "Ship"]);
    const loop = rows.find((r) => r.kind === "mark" && r.role === "loop");
    expect(loop).toMatchObject({ name: "Repeat from Implement until “Checks pass”", line: "max 3" });
    const together = rows.findIndex((r) => r.kind === "mark" && r.role === "together");
    expect(together).toBeGreaterThan(0);
    const smoke = rows.find((r) => r.kind === "step" && r.id === "smoke");
    const impl = rows.find((r) => r.kind === "step" && r.id === "impl");
    expect(smoke && smoke.kind === "step" && smoke.indent).toBe(24);
    expect(impl && impl.kind === "step" && impl.indent).toBe(24);
    const passed = rows.find((r) => r.kind === "step" && r.id === "passed");
    expect(passed && passed.kind === "step" && passed.line).toBe("yes → Smoke test, Summarise changes · no → repeat");
    expect(rows[rows.length - 1]).toMatchObject({ role: "end", name: "PR open", line: "after Open PR" });
  });
});
