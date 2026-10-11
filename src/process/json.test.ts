import { describe, expect, it } from "vitest";
import { BUG_FIX, FEATURE_DELIVERY } from "./examples";
import { errorLine, formatProcess, hashOf, parseProcess, stepAtLine, stepLineRange, tokenize, validateProcess } from "./json";

describe("formatProcess", () => {
  it("keeps short objects on one line and round-trips", () => {
    const text = formatProcess(FEATURE_DELIVERY);
    expect(text).toContain('    { "id": "you", "kind": "you" },');
    expect(text.split("\n").every((l) => l.length <= 96 || !l.trim().startsWith("{ "))).toBe(true);
    const parsed = JSON.parse(text);
    expect(parsed).toEqual(FEATURE_DELIVERY);
    expect(formatProcess(parsed)).toBe(text);
  });

  it("leaves out undefined fields", () => {
    expect(formatProcess({ a: 1, b: undefined })).toBe('{ "a": 1 }');
  });
});

describe("tokenize", () => {
  it("tells keys from string values", () => {
    expect(tokenize('  "id": "plan",').map((t) => t.kind)).toEqual(["plain", "key", "punct", "plain", "string", "punct"]);
    expect(tokenize('"max": 3, "x": true').filter((t) => t.kind === "number" || t.kind === "literal").map((t) => t.text)).toEqual(["3", "true"]);
  });
});

describe("step lines", () => {
  it("finds a step's block and the step under a line", () => {
    const lines = formatProcess(FEATURE_DELIVERY).split("\n");
    const range = stepLineRange(lines, "passed")!;
    expect(range[1]).toBeGreaterThan(range[0]);
    expect(lines[range[0]]!.trim()).toBe("{");
    expect(lines.slice(range[0], range[1] + 1).join("\n")).toContain('"decide": "checks.exitCode == 0"');
    const ids = FEATURE_DELIVERY.steps.map((s) => s.id);
    expect(stepAtLine(lines, range[0] + 1, ids)).toBe("passed");
    expect(stepAtLine(lines, 0, ids)).toBeNull();
    expect(stepLineRange(lines, "missing")).toBeNull();
  });
});

describe("parseProcess", () => {
  it("accepts a valid process", () => {
    const result = parseProcess(formatProcess(BUG_FIX));
    expect(result.ok).toBe(true);
  });

  it("reports the line of a syntax error", () => {
    const text = formatProcess(FEATURE_DELIVERY).replace('"name": "Feature delivery",', '"name": "Feature delivery"');
    const result = parseProcess(text);
    expect(result.ok).toBe(false);
    // The missing comma is noticed at the next key.
    if (!result.ok) expect(result.line).toBe(5);
  });

  it("reads both styles of error position", () => {
    expect(errorLine("Unexpected token } in JSON at position 5", "{\n\n}")).toBe(3);
    expect(errorLine("Expected ',' (line 7 column 3)", "")).toBe(7);
  });

  it("points at a step that refers to a missing step", () => {
    const text = formatProcess(FEATURE_DELIVERY).replace('"after": "impl"', '"after": "implx"');
    const result = parseProcess(text);
    expect(result).toMatchObject({ ok: false, error: 'Step "checks" points at "implx", which is not a step' });
    if (!result.ok) expect(text.split("\n")[result.line! - 1]).toContain('"id": "checks"');
  });
});

describe("validateProcess", () => {
  it("catches the wrong schema, unknown rows and circles", () => {
    expect(validateProcess({ ...FEATURE_DELIVERY, $schema: "x" })[0]!.message).toContain("$schema");
    const offRow = { ...FEATURE_DELIVERY, steps: [{ ...FEATURE_DELIVERY.steps[0]!, on: "robot" }] };
    expect(validateProcess(offRow)[0]!.message).toContain("not a row");
    const loop = {
      ...FEATURE_DELIVERY,
      steps: FEATURE_DELIVERY.steps.map((s) => (s.id === "plan" ? { ...s, after: "pr" } : s)),
    };
    expect(validateProcess(loop).map((p) => p.message)).toContain("Steps wait on each other in a circle");
  });
});

describe("hashOf", () => {
  it("is short and changes with the content", () => {
    const a = hashOf(FEATURE_DELIVERY);
    expect(a).toMatch(/^[0-9a-f]{7}$/);
    expect(hashOf({ ...FEATURE_DELIVERY, name: "Other" })).not.toBe(a);
  });
});
