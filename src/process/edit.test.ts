import { describe, expect, it } from "vitest";
import { addSurface, insertStep, removeStep, renameStage, updateStep } from "./edit";
import { BUG_FIX, FEATURE_DELIVERY } from "./examples";
import { buildGraph } from "./graph";
import { validateProcess } from "./json";

describe("insertStep", () => {
  it("goes between a step and what came after it", () => {
    const { process, id } = insertStep(FEATURE_DELIVERY, "shell", "impl");
    expect(validateProcess(process)).toEqual([]);
    const step = process.steps.find((s) => s.id === id)!;
    expect(step).toMatchObject({ after: "impl", on: "shell", stage: "build", run: "pnpm test" });
    expect(process.steps.find((s) => s.id === "checks")!.after).toBe(id);
    expect(buildGraph(process).col[id]).toBe(3);
  });

  it("goes on the yes side after a decision", () => {
    const { process, id } = insertStep(FEATURE_DELIVERY, "approve", "passed");
    expect(validateProcess(process)).toEqual([]);
    expect(process.steps.find((s) => s.id === "passed")!.yes).toEqual([id]);
    expect(process.steps.find((s) => s.id === "smoke")!.after).toBe(id);
  });

  it("moves the end along when added after the last step", () => {
    const { process, id } = insertStep(FEATURE_DELIVERY, "claude", null);
    expect(process.ends).toEqual([{ after: id, name: "PR open" }]);
    expect(process.steps[process.steps.length - 1]!.id).toBe(id);
  });

  it("hangs the steps that came next off a new decision's yes", () => {
    const { process, id } = insertStep(FEATURE_DELIVERY, "decision", "review");
    expect(validateProcess(process)).toEqual([]);
    expect(process.steps.find((s) => s.id === id)).toMatchObject({ after: "review", yes: ["pr"] });
    expect(process.steps.find((s) => s.id === "pr")!.after).toBeUndefined();
  });

  it("never reuses an id", () => {
    const once = insertStep(FEATURE_DELIVERY, "shell", "impl");
    const twice = insertStep(once.process, "shell", "impl");
    expect(twice.id).not.toBe(once.id);
  });
});

describe("removeStep", () => {
  it("closes the gap", () => {
    const process = removeStep(FEATURE_DELIVERY, "checks");
    expect(validateProcess(process)).toEqual([]);
    expect(process.steps.find((s) => s.id === "passed")!.after).toBe("impl");
  });

  it("drops a loop back to a removed step and moves an end", () => {
    const noImpl = removeStep(FEATURE_DELIVERY, "impl");
    expect(validateProcess(noImpl)).toEqual([]);
    expect(noImpl.steps.find((s) => s.id === "passed")!.no).toEqual([]);
    const noPr = removeStep(FEATURE_DELIVERY, "pr");
    expect(noPr.ends).toEqual([{ after: "review", name: "PR open" }]);
  });

  it("keeps a decision's branch whole when its target goes", () => {
    const process = removeStep(FEATURE_DELIVERY, "smoke");
    expect(validateProcess(process)).toEqual([]);
    expect(process.steps.find((s) => s.id === "passed")!.yes).toEqual(["summary", "review"]);
  });

  it("removes a step from for-each loops", () => {
    const process = removeStep(BUG_FIX, "test");
    expect(validateProcess(process)).toEqual([]);
    expect(process.loops![0]!.steps).toEqual(["fix", "green"]);
    expect(process.steps.find((s) => s.id === "green")!.after).toBe("fix");
  });

  it("re-attaches a removed decision's branches", () => {
    const process = removeStep(BUG_FIX, "found");
    expect(validateProcess(process)).toEqual([]);
    expect(process.steps.find((s) => s.id === "fix")!.after).toBe("repro");
  });
});

describe("small edits", () => {
  it("merge fields and remove the ones set to undefined", () => {
    const p = updateStep(FEATURE_DELIVERY, "plan", { prompt: "New", saves: undefined });
    expect(p.steps[0]).toMatchObject({ prompt: "New" });
    expect("saves" in p.steps[0]!).toBe(false);
    expect(FEATURE_DELIVERY.steps[0]!.saves).toBe("plan.md");
  });

  it("rename stages and add rows", () => {
    expect(renameStage(FEATURE_DELIVERY, "plan", "Think").stages[0]!.name).toBe("Think");
    const rows = addSurface(addSurface(FEATURE_DELIVERY, "terminal"), "terminal");
    expect(rows.surfaces.slice(-2).map((s) => s.name)).toEqual(["Shell 2", "Shell 3"]);
  });
});
