#!/usr/bin/env node
// Appends one event to plans/_runs/activity.jsonl so the dashboard can show
// what the pipeline is doing right now. Skills call this at every step.
//
//   node plans/_tools/log-event.mjs <event> <epic> [card] [--model haiku]
//        [--result pass|fail|blocked] [--note "short text"]
//
// Events: plan-start, plan-end, research-start, research-end, carding-start,
// carding-end, review-start, review-end, build-start, build-end, check-start,
// check-end, retry, escalate, commit, blocked, epic-start, epic-end.
import { appendFileSync, mkdirSync } from 'node:fs';
import { join } from 'node:path';
import { PLANS } from './cards.mjs';

const [event, epic, maybeCard, ...rest] = process.argv.slice(2);
if (!event || !epic) {
  console.error('usage: log-event.mjs <event> <epic> [card] [--model m] [--result r] [--note text]');
  process.exit(2);
}
const args = maybeCard && !maybeCard.startsWith('--') ? rest : [maybeCard, ...rest].filter(Boolean);
const card = maybeCard && !maybeCard.startsWith('--') ? maybeCard : undefined;
const opts = {};
for (let i = 0; i < args.length; i += 2) {
  const key = args[i]?.replace(/^--/, '');
  if (key) opts[key] = args[i + 1];
}

const dir = join(PLANS, '_runs');
mkdirSync(dir, { recursive: true });
const line = { ts: new Date().toISOString(), event, epic, ...(card && { card }), ...opts };
appendFileSync(join(dir, 'activity.jsonl'), JSON.stringify(line) + '\n');
