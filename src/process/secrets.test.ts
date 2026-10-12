import { describe, expect, it } from "vitest";
import { BUG_FIX } from "./examples";
import { cloneProcess } from "./json";
import { maskedAt, matchSecret, parseSecretError, pathKeys, scanSecrets, secretWhat, stepIndexOf, valueAtPath } from "./secrets";

// Built from parts so this file never holds a whole token itself.
const GH = "ghp_" + "a1B2c3D4e5F6g7H8i9J0k1L2m3N4o5P6q7R8";

describe("matchSecret", () => {
  const cases: [string, string][] = [
    ["github-token", `token ${GH}`],
    ["github-fine-grained-pat", "github_pat_" + "11ABCDEFG0123456789_abcdefghijklmnopqrstuvwxyz"],
    ["anthropic-api-key", "sk-ant-" + "api03-abcdefghijklmnopqrstuvwxyz012345"],
    ["openai-api-key", "sk-" + "proj-abcdefghijklmnopqrstuvwxyz0123"],
    ["openai-api-key", "sk-" + "abcdefghijklmnopqrstuvwxyz0123456789ABCDEFGHIJKL"],
    ["slack-token", "xoxb-" + "123456789012-abcdefghijkl"],
    ["slack-webhook", "https://hooks.slack.com/services/" + "T000/B000/XXXXXXXXXXXXXXXX"],
    ["aws-access-key-id", "AKIA" + "IOSFODNN7EXAMPLE"],
    ["stripe-live-key", "sk_live_" + "51HabcdefghijklmnopQRST"],
    ["private-key", "-----BEGIN RSA PRIVATE KEY-----\nMIIE..."],
    ["private-key", "-----BEGIN PRIVATE KEY-----"],
  ];
  it.each(cases)("finds %s", (rule, text) => {
    expect(matchSecret(text)?.rule).toBe(rule);
  });

  it("leaves ordinary text and connection names alone", () => {
    for (const text of ["gh pr create --fill", "${conn:github}", "sk-short", "ask-ant-ignore", "pnpm test", "AKIA lower", "sk_test_51Habcdefghijklmnop"]) {
      expect(matchSecret(text)).toBeNull();
    }
  });
});

describe("scanSecrets", () => {
  it("is empty for the example processes", () => {
    expect(scanSecrets(BUG_FIX)).toEqual([]);
  });

  it("reports the server's path shape and a masked value", () => {
    const p = cloneProcess(BUG_FIX) as unknown as Record<string, unknown>;
    p.lanes = [{}, {}, { steps: [{}, { env: { GH_TOKEN: GH } }] }];
    expect(scanSecrets(p)).toEqual([{ rule: "github-token", path: "lanes[2].steps[1].env.GH_TOKEN", masked: "ghp_••••••••••••" }]);
  });

  it("quotes keys that aren't plain names", () => {
    expect(scanSecrets({ env: { "x-key": "AKIA" + "IOSFODNN7EXAMPLE" } })[0]!.path).toBe('env["x-key"]');
  });

  it("finds a token inside a longer prompt", () => {
    const p = cloneProcess(BUG_FIX);
    p.steps[0]!.prompt = `Use ${GH} to read the repo`;
    const [hit] = scanSecrets(p);
    expect(hit!.path).toBe("steps[0].prompt");
    expect(stepIndexOf(hit!.path)).toBe(0);
  });
});

describe("parseSecretError", () => {
  it("reads the rule and drops the $.", () => {
    expect(parseSecretError("definition_contains_secret: github-token at $.lanes[0].stages[0].steps[0].prompt")).toEqual({
      rule: "github-token",
      path: "lanes[0].stages[0].steps[0].prompt",
    });
  });

  it("ignores other 422s", () => {
    expect(parseSecretError("definition is not a process")).toBeNull();
  });
});

describe("paths", () => {
  it("splits and resolves", () => {
    expect(pathKeys('lanes[2].env["X-Y"].a')).toEqual(["lanes", 2, "env", "X-Y", "a"]);
    const value = { steps: [{ env: { GH_TOKEN: GH } }] };
    expect(valueAtPath(value, "steps[0].env.GH_TOKEN")).toBe(GH);
    expect(valueAtPath(value, "steps[3].env")).toBeUndefined();
    expect(maskedAt(value, "steps[0].env.GH_TOKEN")).toBe("ghp_••••••••••••");
    expect(maskedAt(value, "steps[0].env")).toBeNull();
    expect(stepIndexOf("lanes[0].steps[1]")).toBeNull();
  });

  it("words a rule", () => {
    expect(secretWhat("github-token")).toBe("a GitHub token");
    expect(secretWhat("something-new")).toBe("a key or token");
  });
});
