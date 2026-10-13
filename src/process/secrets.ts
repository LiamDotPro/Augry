/**
 * The secret check run before a process goes to a team (Process board 11,
 * share-blocked). Team processes can't hold keys or tokens; the server scans
 * every definition and refuses a match with a 422
 * ("definition_contains_secret: <rule> at <json path>"). This is the server's
 * scan, rule for rule (Terminal-Grid-Cloud apps/api/internal/processes/
 * secrets.go): the same patterns in the same order, every string value and
 * every object key, keys in sorted order, and the same paths. So the dialog
 * shows what the server would refuse, before anything is sent, and nothing
 * it would accept.
 */

export type SecretRule =
  | "github-token"
  | "github-fine-grained-pat"
  | "anthropic-api-key"
  | "openai-api-key"
  | "slack-token"
  | "slack-webhook"
  | "aws-access-key-id"
  | "stripe-live-key"
  | "private-key";

interface RuleDef {
  id: SecretRule;
  pattern: RegExp;
  /** The part of a match that is safe to show: the token's well-known prefix. */
  prefix: RegExp;
  /** "Bug fix has what looks like <what>." */
  what: string;
}

// The server's rules, in its order: the first match is the one it reports.
const RULES: RuleDef[] = [
  { id: "github-token", pattern: /\b(?:ghp|gho|ghu|ghs|ghr)_[0-9A-Za-z]{36}\b/, prefix: /^gh[pousr]_/, what: "a GitHub token" },
  { id: "github-fine-grained-pat", pattern: /\bgithub_pat_\w{82}\b/, prefix: /^github_pat_/, what: "a GitHub token" },
  { id: "anthropic-api-key", pattern: /\bsk-ant-(?:api|admin)\d{2}-[A-Za-z0-9_-]{80,}/, prefix: /^sk-ant-(?:api|admin)\d{2}-/, what: "an Anthropic API key" },
  {
    id: "openai-api-key",
    pattern: /\bsk-(?:proj|svcacct|admin)-[A-Za-z0-9_-]{40,}|\bsk-[A-Za-z0-9]{20}T3BlbkFJ[A-Za-z0-9]{20}\b/,
    prefix: /^sk-(?:proj-|svcacct-|admin-)?/,
    what: "an OpenAI API key",
  },
  { id: "slack-token", pattern: /\bxox[bpsar]-\d{8,}-[0-9A-Za-z-]{10,}|\bxoxe(?:\.xox[bp])?-\d-[A-Za-z0-9]{100,}/, prefix: /^xox[a-z.]*-/, what: "a Slack token" },
  { id: "slack-webhook", pattern: /hooks\.slack\.com\/(?:services|workflows|triggers)\/[A-Za-z0-9+/]{43,56}/, prefix: /^hooks\.slack\.com\/[a-z]+\//, what: "a Slack webhook" },
  { id: "aws-access-key-id", pattern: /\b(?:AKIA|ASIA)[A-Z2-7]{16}\b/, prefix: /^[A-Z]{4}/, what: "an AWS access key" },
  { id: "stripe-live-key", pattern: /\b(?:sk|rk)_live_[0-9A-Za-z]{24,99}\b/, prefix: /^[sr]k_live_/, what: "a Stripe live key" },
  { id: "private-key", pattern: /-----BEGIN (?:[A-Z0-9]+ )*PRIVATE KEY(?: BLOCK)?-----/, prefix: /^-----BEGIN [A-Z0-9 ]*PRIVATE KEY(?: BLOCK)?-----/, what: "a private key" },
];

export interface SecretFinding {
  rule: SecretRule;
  /**
   * Where it is, as the server writes it without the leading "$" or "$.":
   * steps[1].env.GH_TOKEN, or steps[1].env["<redacted key>"] for a key.
   */
  path: string;
  /** The token with all but its prefix hidden: ghp_•••••••••••• */
  masked: string;
}

const BULLETS = "•".repeat(12);

/** Hides a token but its well-known prefix. */
export function maskSecret(rule: SecretRule | string, token: string): string {
  const def = RULES.find((r) => r.id === rule);
  const prefix = def ? (token.match(def.prefix)?.[0] ?? "") : token.slice(0, 4);
  return `${prefix}${BULLETS}`;
}

/** What the share-blocked text calls a rule's match: "a GitHub token". */
export function secretWhat(rule: string): string {
  return RULES.find((r) => r.id === rule)?.what ?? "a key or token";
}

/** Characters that make the server quote a key: `["a b"]` rather than `.ab`. */
const QUOTE = /[.[\]"' $\\]/;

/** The server's keyPath: `$.key`, or `$["a key"]` when it needs quoting. */
function keyPath(path: string, key: string): string {
  return key !== "" && !QUOTE.test(key) ? `${path}.${key}` : `${path}[${JSON.stringify(key)}]`;
}

/** "$.steps[1].run" as the dialog shows it: "steps[1].run". */
const shortPath = (path: string) => path.replace(/^\$\.?/, "");

/** The first rule a string breaks, with the matching token. */
export function matchSecret(text: string): { rule: SecretRule; token: string } | null {
  for (const r of RULES) {
    const m = text.match(r.pattern);
    if (m) return { rule: r.id, token: m[0] };
  }
  return null;
}

/** Every key or token in `value`, in the order the server finds them. */
export function scanSecrets(value: unknown): SecretFinding[] {
  const out: SecretFinding[] = [];
  const match = (text: string, path: string) => {
    for (const r of RULES) {
      const m = text.match(r.pattern);
      if (m) out.push({ rule: r.id, path, masked: maskSecret(r.id, m[0]) });
    }
  };
  const walk = (v: unknown, path: string) => {
    if (typeof v === "string") match(v, shortPath(path));
    else if (Array.isArray(v)) v.forEach((x, i) => walk(x, `${path}[${i}]`));
    else if (v && typeof v === "object") {
      const entries = v as Record<string, unknown>;
      for (const key of Object.keys(entries).sort()) {
        let p = keyPath(path, key);
        const before = out.length;
        match(key, shortPath(p));
        if (out.length > before) {
          // A secret used as a key is still a secret: the path doesn't echo it.
          p = `${path}["<redacted key>"]`;
          for (let i = before; i < out.length; i++) out[i] = { ...out[i]!, path: shortPath(p) };
        }
        walk(entries[key], p);
      }
    }
  };
  walk(value, "$");
  return out;
}

/**
 * The server's 422 message, "definition_contains_secret: github-token at
 * $.lanes[0].steps[0].prompt", as a rule and a path without the "$.".
 */
export function parseSecretError(message: string): { rule: string; path: string } | null {
  // The path runs to the end: a redacted key's path has a space in it.
  const m = message.match(/^definition_contains_secret:\s*([\w-]+)\s+at\s+(.+)$/);
  if (!m) return null;
  return { rule: m[1]!, path: m[2]!.replace(/^\$\.?/, "") };
}

/**
 * Splits a path, as the server writes it, into its keys:
 * `steps[1].env["X Y"].1st` → ["steps", 1, "env", "X Y", "1st"].
 */
export function pathKeys(path: string): (string | number)[] {
  const keys: (string | number)[] = [];
  const re = /\[(\d+)\]|\[("(?:[^"\\]|\\.)*")\]|\.?([^.[\]"' $\\]+)/g;
  let m: RegExpExecArray | null;
  while ((m = re.exec(path.replace(/^\$/, "")))) {
    if (m[1] !== undefined) keys.push(Number(m[1]));
    else if (m[2] !== undefined) keys.push(JSON.parse(m[2]) as string);
    else if (m[3] !== undefined) keys.push(m[3]);
  }
  return keys;
}

/** The value at a path, or undefined. */
export function valueAtPath(value: unknown, path: string): unknown {
  let v: unknown = value;
  for (const key of pathKeys(path)) {
    if (v == null || typeof v !== "object") return undefined;
    v = (v as Record<string | number, unknown>)[key];
  }
  return v;
}

/** The masked value at a path the server reported, if it still holds a match there. */
export function maskedAt(value: unknown, path: string): string | null {
  const v = valueAtPath(value, path);
  if (typeof v !== "string") return null;
  const hit = matchSecret(v);
  return hit ? maskSecret(hit.rule, hit.token) : null;
}

/** The index of the step a path points into (`steps[3].prompt` → 3), to select it in the JSON panel. */
export function stepIndexOf(path: string): number | null {
  const keys = pathKeys(path);
  return keys[0] === "steps" && typeof keys[1] === "number" ? keys[1] : null;
}
