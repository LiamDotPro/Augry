/**
 * The secret check run before a process goes to a team (Process board 11,
 * share-blocked). Team processes can't hold keys or tokens; the server scans
 * every definition with these same rule ids and refuses a match with a 422
 * ("definition_contains_secret: <rule> at <json path>"). Running it here
 * first shows the dialog without a round trip, and the server's message is
 * parsed into the same shape when it still finds something.
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

// Order matters: the more specific rules come first, so an Anthropic key is
// never reported as an OpenAI one.
const RULES: RuleDef[] = [
  { id: "private-key", pattern: /-----BEGIN (?:[A-Z0-9]+ )*PRIVATE KEY(?: BLOCK)?-----/, prefix: /^-----BEGIN [A-Z0-9 ]*PRIVATE KEY(?: BLOCK)?-----/, what: "a private key" },
  { id: "github-fine-grained-pat", pattern: /\bgithub_pat_[A-Za-z0-9_]{22,}/, prefix: /^github_pat_/, what: "a GitHub token" },
  { id: "github-token", pattern: /\bgh[pousr]_[A-Za-z0-9]{30,}/, prefix: /^gh[pousr]_/, what: "a GitHub token" },
  { id: "anthropic-api-key", pattern: /\bsk-ant-[A-Za-z0-9_-]{20,}/, prefix: /^sk-ant-(?:[a-z]+\d*-)?/, what: "an Anthropic API key" },
  { id: "openai-api-key", pattern: /\bsk-(?!ant-)(?:proj-|svcacct-|admin-)?[A-Za-z0-9_-]{20,}/, prefix: /^sk-(?:proj-|svcacct-|admin-)?/, what: "an OpenAI API key" },
  { id: "slack-webhook", pattern: /https:\/\/hooks\.slack\.com\/(?:services|workflows|triggers)\/[A-Za-z0-9_/-]{10,}/, prefix: /^https:\/\/hooks\.slack\.com\/[a-z]+\//, what: "a Slack webhook" },
  { id: "slack-token", pattern: /\bxox[abposre]-[A-Za-z0-9-]{10,}/, prefix: /^xox[a-z]-/, what: "a Slack token" },
  { id: "aws-access-key-id", pattern: /\b(?:AKIA|ASIA|ABIA|ACCA)[A-Z0-9]{16}\b/, prefix: /^[A-Z]{4}/, what: "an AWS access key" },
  { id: "stripe-live-key", pattern: /\b(?:sk|rk)_live_[A-Za-z0-9]{16,}/, prefix: /^[sr]k_live_/, what: "a Stripe live key" },
];

export interface SecretFinding {
  rule: SecretRule;
  /** Where it is, as the server writes it without the leading "$.": lanes[2].steps[1].env.GH_TOKEN */
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

const IDENT = /^[A-Za-z_$][\w$]*$/;

function child(path: string, key: string | number): string {
  if (typeof key === "number") return `${path}[${key}]`;
  if (IDENT.test(key)) return path ? `${path}.${key}` : key;
  return `${path}[${JSON.stringify(key)}]`;
}

/** The first rule a string breaks, with the matching token. */
export function matchSecret(text: string): { rule: SecretRule; token: string } | null {
  for (const r of RULES) {
    const m = text.match(r.pattern);
    if (m) return { rule: r.id, token: m[0] };
  }
  return null;
}

/** Every string in `value` that looks like a key or token, in document order. */
export function scanSecrets(value: unknown): SecretFinding[] {
  const out: SecretFinding[] = [];
  const walk = (v: unknown, path: string) => {
    if (typeof v === "string") {
      const hit = matchSecret(v);
      if (hit) out.push({ rule: hit.rule, path, masked: maskSecret(hit.rule, hit.token) });
    } else if (Array.isArray(v)) {
      v.forEach((x, i) => walk(x, child(path, i)));
    } else if (v && typeof v === "object") {
      for (const [k, x] of Object.entries(v)) walk(x, child(path, k));
    }
  };
  walk(value, "");
  return out;
}

/**
 * The server's 422 message, "definition_contains_secret: github-token at
 * $.lanes[0].steps[0].prompt", as a rule and a path without the "$.".
 */
export function parseSecretError(message: string): { rule: string; path: string } | null {
  const m = message.match(/^definition_contains_secret:\s*([\w-]+)\s+at\s+(\S+)/);
  if (!m) return null;
  return { rule: m[1]!, path: m[2]!.replace(/^\$\.?/, "") };
}

/** Splits a path into its keys: `steps[1].env["X-Y"]` → ["steps", 1, "env", "X-Y"]. */
export function pathKeys(path: string): (string | number)[] {
  const keys: (string | number)[] = [];
  const re = /([A-Za-z_$][\w$-]*)|\[(\d+)\]|\[("(?:[^"\\]|\\.)*")\]/g;
  let m: RegExpExecArray | null;
  while ((m = re.exec(path))) {
    if (m[1] !== undefined) keys.push(m[1]);
    else if (m[2] !== undefined) keys.push(Number(m[2]));
    else keys.push(JSON.parse(m[3]!) as string);
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
