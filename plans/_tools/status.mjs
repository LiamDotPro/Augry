#!/usr/bin/env node
// Prints every card and its status, for one epic or all of them.
//   node plans/_tools/status.mjs [epic]
import { listEpics, loadEpic } from './cards.mjs';

const only = process.argv[2];
const epics = only ? [only] : listEpics();
const totals = {};

for (const id of epics) {
  const { meta, cards } = loadEpic(id);
  console.log(`\n${id}  [${meta?.status ?? 'no epic.md'}]  ${meta?.title ?? ''}`);
  if (!cards.length) {
    console.log('  (no cards yet)');
    continue;
  }
  const done = new Set(cards.filter((c) => c.data.status === 'done').map((c) => c.data.id));
  for (const c of cards) {
    const d = c.data;
    const deps = d.depends_on ?? [];
    const waiting = deps.filter((x) => !x.includes('/') && !done.has(x));
    const ready = d.status === 'todo' && waiting.length === 0 ? ' ready' : '';
    totals[d.status] = (totals[d.status] ?? 0) + 1;
    console.log(
      `  ${String(d.id).padEnd(4)} ${String(d.status).padEnd(11)} ${String(d.size ?? '').padEnd(2)} ${String(d.model ?? 'haiku').padEnd(6)} ${d.title}${ready}${waiting.length && d.status === 'todo' ? `  (waits on ${waiting.join(', ')})` : ''}`,
    );
  }
}
console.log(
  '\n' + Object.entries(totals).map(([k, v]) => `${k}: ${v}`).join('  '),
);
