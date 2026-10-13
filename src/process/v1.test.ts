import { describe, expect, it } from "vitest";
import { isV1, processFromV1 } from "./v1";

// The server's starter templates, as GET /v1/templates/{slug} returns them.
const REVIEW = {
  schemaVersion: 1,
  name: "Review my changes",
  description: "Get an agent review of your uncommitted changes before you commit.",
  requires: ["git", "claude"],
  lanes: [
    {
      id: "reviewer",
      title: "Reviewer",
      stages: [
        {
          id: "review",
          title: "Review",
          steps: [
            { id: "diff", title: "Show what changed", kind: "shell", command: "git status --short && git diff --stat" },
            { id: "findings", title: "Review the diff", kind: "agent", agent: "claude", prompt: "Review the changes." },
          ],
        },
      ],
    },
  ],
};

const SECOND_OPINION = {
  schemaVersion: 1,
  name: "Second opinion",
  lanes: [
    { id: "claude", title: "Claude", stages: [{ id: "propose", title: "Propose", steps: [{ id: "plan", kind: "agent", agent: "claude", prompt: "A" }] }] },
    { id: "codex", title: "Codex", stages: [{ id: "propose", title: "Propose", steps: [{ id: "plan", kind: "agent", agent: "codex", prompt: "B" }] }] },
  ],
};

describe("processFromV1", () => {
  it("makes each lane a row whose steps run in order", () => {
    const process = processFromV1(REVIEW, { id: "review-my-changes", from: "template:review-changes@1" })!;
    expect(process).not.toBeNull();
    expect(process.name).toBe("Review my changes");
    expect(process.from).toBe("template:review-changes@1");
    expect(process.start).toEqual({ on: "manual" });
    expect(process.surfaces).toEqual([
      { id: "you", kind: "you" },
      { id: "reviewer", kind: "terminal", agent: "claude-code", cwd: "repo", name: "Reviewer" },
    ]);
    expect(process.steps).toEqual([
      { id: "diff", name: "Show what changed", stage: "review", on: "reviewer", run: "git status --short && git diff --stat" },
      { id: "findings", name: "Review the diff", stage: "review", on: "reviewer", prompt: "Review the changes.", after: "diff" },
    ]);
  });

  it("keeps ids unique across lanes and picks each lane's agent", () => {
    const process = processFromV1(SECOND_OPINION, { id: "second-opinion" })!;
    expect(process.surfaces.map((s) => s.agent)).toEqual([undefined, "claude-code", "codex"]);
    expect(process.stages.map((s) => s.id)).toEqual(["propose", "codex-propose"]);
    expect(process.steps.map((s) => [s.id, s.stage, s.on])).toEqual([
      ["plan", "propose", "claude"],
      ["codex-plan", "codex-propose", "codex"],
    ]);
  });

  it("leaves our own format and broken definitions alone", () => {
    expect(isV1({ schemaVersion: 2, name: "x", lanes: [] })).toBe(false);
    expect(processFromV1({ schemaVersion: 1, name: "x", lanes: [{ id: "a" }] }, { id: "x" })).toBeNull();
    expect(processFromV1({ name: "x" }, { id: "x" })).toBeNull();
  });
});
