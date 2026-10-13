import { describe, expect, it } from "vitest";
import { BUG_FIX } from "./examples";
import { cloneProcess } from "./json";
import { maskedAt, matchSecret, parseSecretError, pathKeys, scanSecrets, secretWhat, stepIndexOf, valueAtPath } from "./secrets";

// Built from parts so this file never holds a whole token itself.
const a = (c: string, n: number) => c.repeat(n);
const GH = "ghp_" + "a1B2c3D4e5F6g7H8i9J0k1L2m3N4o5P6q7R8";

// The server's own samples (secrets_test.go), so both scans are held to the same inputs.
describe("matchSecret", () => {
  const cases: [string, string][] = [
    ["github-token", `curl -H 'Authorization: token ghp_${a("a", 36)}'`],
    ["github-token", "gho_" + a("B", 36)],
    ["github-token", "ghs_" + a("1", 36)],
    ["github-fine-grained-pat", "github_pat_" + a("x", 82)],
    ["anthropic-api-key", "sk-ant-api03-" + a("q", 93) + "AA"],
    ["openai-api-key", "sk-proj-" + a("z", 60)],
    ["openai-api-key", "sk-" + a("c", 20) + "T3BlbkFJ" + a("d", 20)],
    ["slack-token", "xoxb-781236542736-2364535789652-" + "GkwFDQoHqzXDVsC6GzqYUypD"],
    ["slack-webhook", "https://hooks.slack.com/services/" + a("T", 43)],
    ["slack-webhook", "hooks.slack.com/services/" + a("T", 43)],
    ["aws-access-key-id", "AKIA" + "IOSFODNN7EXAMPLE"],
    ["stripe-live-key", "sk_live_" + a("k", 24)],
    ["private-key", "-----BEGIN OPENSSH PRIVATE KEY-----\nAAAA"],
    ["private-key", "-----BEGIN RSA PRIVATE KEY-----"],
  ];
  it.each(cases)("finds %s", (rule, text) => {
    expect(matchSecret(text)?.rule).toBe(rule);
  });

  it("leaves alone what the server accepts", () => {
    for (const text of [
      "${env:GITHUB_TOKEN}",
      "${conn:github}",
      "pip install scikit-learn",
      "task-runner-with-a-very-long-name-1234567890123456789",
      "sk-short",
      "ghp_tooshort",
      "AKIA1234",
      "sk_test_" + a("k", 30),
      "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789+/abcdef",
      "git rev-parse HEAD 4b825dc642cb6eb9a060e54bf8d69288fbee4904",
      "-----BEGIN CERTIFICATE-----",
      // Looser shapes the scan used to block; the server takes them.
      "ghp_" + a("a", 30),
      "sk-" + a("b", 30),
    ]) {
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

  it("writes paths as the server does", () => {
    expect(scanSecrets({ lanes: [{ x: { "API KEY": "AKIA" + "IOSFODNN7EXAMPLE" } }] })[0]!.path).toBe('lanes[0].x["API KEY"]');
    expect(scanSecrets({ env: { "x-key": "AKIA" + "IOSFODNN7EXAMPLE" } })[0]!.path).toBe("env.x-key");
  });

  it("finds a token used as a key, without echoing it", () => {
    expect(scanSecrets({ env: { [GH]: 1 } })).toEqual([{ rule: "github-token", path: 'env["<redacted key>"]', masked: "ghp_••••••••••••" }]);
  });

  it("reports in the server's order: keys sorted, rules in order", () => {
    const found = scanSecrets({ b: "AKIA" + "IOSFODNN7EXAMPLE", a: GH });
    expect(found.map((f) => f.path)).toEqual(["a", "b"]);
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

  it("keeps a redacted key's path whole", () => {
    expect(parseSecretError('definition_contains_secret: github-token at $.env["<redacted key>"]')).toEqual({
      rule: "github-token",
      path: 'env["<redacted key>"]',
    });
  });

  it("ignores other 422s", () => {
    expect(parseSecretError("definition is not a process")).toBeNull();
  });
});

describe("paths", () => {
  it("splits and resolves", () => {
    expect(pathKeys('lanes[2].env["X Y"].a')).toEqual(["lanes", 2, "env", "X Y", "a"]);
    expect(pathKeys("$.env.x-key.1st")).toEqual(["env", "x-key", "1st"]);
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
