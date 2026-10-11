import { describe, expect, it } from "vitest";
import { BUG_FIX, FEATURE_DELIVERY } from "./examples";
import { layoutLanes } from "./layout";
import { advanceRun, approveRun, laneStatus, runHeadline, sendBackRun, startRun, stopRun, waitingStep, agoShort, whenLabel } from "./runs";
import type { Run } from "./types";

const T0 = new Date(2026, 9, 11, 10, 31).getTime();

/** Ticks until the run waits or ends, approving nothing. */
function untilStill(run: Run, max = 40): Run {
  let r = run;
  for (let i = 0; i < max; i++) {
    const next = advanceRun(r, T0 + i * 1000);
    if (next === r) return r;
    r = next;
  }
  return r;
}

describe("a run of Feature delivery", () => {
  it("starts on the first step and stops for you at Approve plan", () => {
    const run = startRun(FEATURE_DELIVERY, { number: 5, version: 7, trigger: "issue #142", now: T0 });
    expect(run.steps.plan).toEqual({ state: "running", note: "" });
    expect(run.status).toBe("running");
    const waiting = untilStill(run);
    expect(waiting.status).toBe("waiting");
    expect(waitingStep(waiting)).toBe("approve");
    expect(runHeadline(waiting)).toBe("Waiting for you at Approve plan");
  });

  it("goes round the checks loop once, then reaches Review changes", () => {
    let run = untilStill(startRun(FEATURE_DELIVERY, { number: 5, version: 7, trigger: "issue #142", now: T0 }));
    run = approveRun(run, "approve", T0);
    expect(run.steps.impl!.state).toBe("running");
    run = untilStill(run);
    expect(run.loops.passed).toBe("ran twice");
    expect(waitingStep(run)).toBe("review");
    expect(run.steps.smoke!.state).toBe("done");
    expect(run.steps.summary!.state).toBe("done");
    run = approveRun(run, "review", T0);
    run = untilStill(run);
    expect(run.status).toBe("done");
    expect(run.result).toBe("PR open");
  });

  it("send back runs the steps before again", () => {
    let run = untilStill(startRun(FEATURE_DELIVERY, { number: 5, version: 7, trigger: "issue #142", now: T0 }));
    run = sendBackRun(run, "approve", T0);
    expect(run.steps.plan).toEqual({ state: "running", note: "sent back" });
    expect(run.steps.approve!.state).toBe("pending");
    expect(run.status).toBe("running");
  });

  it("stop marks what was live", () => {
    const run = stopRun(startRun(FEATURE_DELIVERY, { number: 5, version: 7, trigger: "x", now: T0 }), T0 + 5000);
    expect(run.status).toBe("stopped");
    expect(run.steps.plan!.state).toBe("stopped");
    expect(advanceRun(run, T0 + 9000)).toBe(run);
  });

  it("pins the version it started with", () => {
    const run = startRun(FEATURE_DELIVERY, { number: 5, version: 7, trigger: "x", now: T0 });
    expect(run.process).not.toBe(FEATURE_DELIVERY);
    expect(run.process).toEqual(FEATURE_DELIVERY);
    expect(run.hash).toMatch(/^[0-9a-f]{7}$/);
  });
});

describe("a run of Bug fix", () => {
  it("skips the no branch and goes round each failing test", () => {
    let run = startRun(BUG_FIX, { number: 13, version: 3, trigger: "webhook", now: T0 });
    run = advanceRun(run, T0); // Reproduce done
    run = advanceRun(run, T0); // Reproduced? yes
    expect(run.steps.info!.state).toBe("skipped");
    expect(run.steps.fix!.state).toBe("running");
    expect(run.loops.each).toBe("test 1 of 3");
    run = untilStill(run);
    expect(run.loops.each).toBe("all 3 done");
    expect(waitingStep(run)).toBe("review");
    const lanes = layoutLanes(run.process, { run });
    expect(lanes.ends.find((e) => e.name === "Needs info")!.dim).toBe(true);
    expect(lanes.wait?.stepId).toBe("review");
  });

  it("describes each row", () => {
    const run = startRun(BUG_FIX, { number: 13, version: 3, trigger: "webhook", now: T0, panes: { claude: 3, codex: 2, shell: 4 } });
    const lanes = laneStatus(run);
    expect(lanes.claude).toEqual({ text: "Pane 3 · reproduce", tone: "accent", pane: 3 });
    expect(lanes.you).toEqual({ text: "nothing waiting", tone: "muted", pane: null });
    expect(lanes.browser!.text).toBe("Chrome · not opened yet");
  });
});

describe("time labels", () => {
  it("are short", () => {
    expect(agoShort(T0, T0 + 29 * 60000)).toBe("29m");
    expect(agoShort(T0, T0 + 3 * 3600000)).toBe("3h");
    expect(whenLabel(T0, T0 + 60000)).toBe("10:31");
    expect(whenLabel(T0, T0 + 86400000)).toBe("yesterday");
  });
});
