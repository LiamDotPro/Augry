#!/usr/bin/env node
// Structural checks for task cards, so problems are caught before a builder
// sees them. It cannot judge whether a card is good, only whether it is whole.
//   node plans/_tools/lint-cards.mjs [epic]
import { existsSync } from 'node:fs';
import { join } from 'node:path';
import { ROOT, listEpics, loadEpic } from './cards.mjs';

const REQUIRED_KEYS = ['id', 'epic', 'title', 'status', 'depends_on', 'size', 'model', 'files'];
const STATUSES = ['todo', 'in-progress', 'done', 'blocked'];
const SECTIONS = ['Goal', 'Read first', 'Steps', 'Acceptance', 'Out of scope', 'If stuck', 'Log'];
const VAGUE = /\b(as needed|appropriately|if necessary|etc\.|and so on|TBD|TODO|figure out|decide whether|choose (a|an|the|between)|something like|as appropriate|handle (errors|edge cases) properly)\b/i;

const only = process.argv[2];
const epics = only ? [only] : listEpics();
let errors = 0;
let warnings = 0;

const err = (card, msg) => {
  errors++;
  console.log(`ERROR ${card}: ${msg}`);
};
const warn = (card, msg) => {
  warnings++;
  console.log(`warn  ${card}: ${msg}`);
};

for (const epicId of epics) {
  const { cards, meta } = loadEpic(epicId);
  if (!meta) err(epicId, 'missing epic.md or its front matter');

  const allIds = new Map(cards.map((c) => [c.data.id, c]));
  // Cards may depend on any card in the epic, including a higher id added
  // later; build order follows depends_on. Only cycles are an error.
  const localDeps = (id) => (allIds.get(id)?.data.depends_on ?? []).filter((x) => !x.includes('/'));
  const upstream = (id) => {
    const out = new Set();
    const walk = (x) => localDeps(x).forEach((y) => !out.has(y) && (out.add(y), walk(y)));
    walk(id);
    return out;
  };
  const reaches = (from, to) => upstream(from).has(to);
  const upstreamFiles = upstreamEpics(epicId).flatMap((e) =>
    loadEpic(e).cards.flatMap((c) => c.data.files ?? []),
  );
  const seen = new Set();

  for (const c of cards) {
    const name = `${epicId}/${c.file}`;
    const d = c.data;
    if (!Object.keys(d).length) {
      err(name, 'no front matter');
      continue;
    }
    for (const k of REQUIRED_KEYS) if (!(k in d)) err(name, `front matter missing "${k}"`);
    if (d.id && !c.file.startsWith(d.id + '-')) err(name, `file name should start with "${d.id}-"`);
    if (seen.has(d.id)) err(name, `duplicate id ${d.id}`);
    seen.add(d.id);
    if (d.epic && d.epic !== epicId) err(name, `epic is "${d.epic}", folder is "${epicId}"`);
    if (d.status && !STATUSES.includes(d.status)) err(name, `unknown status "${d.status}"`);
    if (d.size && !['S', 'M'].includes(d.size)) err(name, `size must be S or M (split larger cards)`);
    if (d.model && !['haiku', 'sonnet'].includes(d.model)) err(name, `model must be haiku or sonnet`);

    if (Array.isArray(d.files) && d.files.length === 0) err(name, 'files: is empty');

    for (const dep of d.depends_on ?? []) {
      if (dep.includes('/')) {
        const [otherEpic, otherId] = dep.split('/');
        const other = loadEpic(otherEpic).cards.find((x) => x.data.id === otherId);
        if (!other) err(name, `depends_on ${dep} does not exist`);
      } else if (!allIds.has(dep)) {
        err(name, `depends_on ${dep} does not exist in this epic`);
      } else if (dep === d.id) {
        err(name, `depends_on itself`);
      } else if (reaches(dep, d.id)) {
        err(name, `depends_on ${dep} makes a cycle`);
      }
    }

    for (const s of SECTIONS) {
      if (!new RegExp(`^## ${s}\\s*$`, 'm').test(c.body)) err(name, `missing section "## ${s}"`);
    }
    const section = (s) => {
      const m = c.body.match(new RegExp(`^## ${s}\\s*$([\\s\\S]*?)(?=^## |$(?![\\s\\S]))`, 'm'));
      return m ? m[1].trim() : '';
    };
    const steps = section('Steps');
    if (steps && !/^\d+\./m.test(steps)) err(name, 'Steps must be a numbered list');
    const acceptance = section('Acceptance');
    if (acceptance && !/`[^`]+`/.test(acceptance)) err(name, 'Acceptance has no runnable command in backticks');
    if (acceptance && !/git status --porcelain/.test(acceptance)) warn(name, 'Acceptance has no scope check (git status --porcelain)');
    if (!/blocked/i.test(section('If stuck'))) warn(name, 'If stuck does not say when to set status blocked');

    const vague = (steps.match(new RegExp(VAGUE.source, 'gi')) ?? []);
    if (vague.length) warn(name, `vague wording in Steps: ${[...new Set(vague)].join(', ')}`);

    // "Read first" paths must exist now, or be created by an earlier card in
    // this epic or by any card of an epic this one depends on.
    const readFirst = section('Read first');
    const known = new Set([
      ...cards.filter((x) => x.data.id < d.id || upstream(d.id).has(x.data.id)).flatMap((x) => x.data.files ?? []),
      ...upstreamFiles,
    ]);
    const createdLater = (p) =>
      [...known].some((k) => k === p || k.startsWith(p.endsWith('/') ? p : p + '/') || (k.endsWith('/') && p.startsWith(k)));
    for (const m of readFirst.matchAll(/`([^`\s]+?)(?::\d+(?:-\d+)?)?`/g)) {
      const p = m[1];
      if (!p.includes('/') || p.startsWith('http')) continue;
      if (!existsSync(join(ROOT, p)) && !createdLater(p)) {
        warn(name, `Read first path not found and not created by an earlier card: ${p}`);
      }
    }
  }
}

function upstreamEpics(epicId, seen = new Set()) {
  for (const dep of loadEpic(epicId).meta?.depends_on ?? []) {
    if (!seen.has(dep)) {
      seen.add(dep);
      upstreamEpics(dep, seen);
    }
  }
  return [...seen];
}

console.log(`\n${errors} error(s), ${warnings} warning(s)`);
process.exit(errors ? 1 : 0);
