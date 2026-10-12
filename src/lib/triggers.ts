import type { TriggerFiredEvent, TriggerProcessStatus, TriggerRuleState } from "../ipc/types";

/** How long the notice toast stays before it goes by itself. */
export const NOTICE_MS = 6000;

/** The toast text for a fired trigger, for example "Trigger fs.file-saved fired for Keep the docs tidy". */
export function firedNotice(event: TriggerFiredEvent): string {
  const text = `Trigger ${event.kind} fired for ${event.processName}`;
  return event.coalesced > 0 ? `${text} (and ${event.coalesced} more)` : text;
}

const STATE_WORDS: Record<Exclude<TriggerRuleState, "active">, string> = {
  rejected: "rejected",
  sourceFailed: "could not start",
  skipped: "skipped",
};

/** A rule id is `<processId>/<triggerId>` with an optional `#<n>`; the list shows the part after the slash. */
function triggerLabel(ruleId: string): string {
  const slash = ruleId.indexOf("/");
  return slash === -1 ? ruleId : ruleId.slice(slash + 1);
}

/**
 * The one line under a process in Settings: the problem with its file, or
 * whether it is armed, or how many of its rules run and why the others do not.
 */
export function processStatusLine(process: TriggerProcessStatus): string {
  if (process.problem) return process.problem;
  if (!process.armed) return "Not armed";
  if (process.rules.length === 0) return "Armed, no triggers";

  const parts: string[] = [];
  const active = process.rules.filter((rule) => rule.state === "active").length;
  if (active > 0) parts.push(active === 1 ? "1 rule active" : `${active} rules active`);
  for (const rule of process.rules) {
    if (rule.state === "active") continue;
    parts.push(`${triggerLabel(rule.id)} ${STATE_WORDS[rule.state]}: ${rule.reason ?? "no reason given"}`);
  }
  return parts.join("; ");
}
