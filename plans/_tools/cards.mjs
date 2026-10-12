// Shared helpers for the plan tools. No dependencies: the front matter used in
// plans/ is a small subset of YAML (scalars, inline [] lists, block "- x"
// lists, and "# comments"), parsed here by hand.
import { readFileSync, readdirSync, existsSync } from 'node:fs';
import { join, dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { execFileSync } from 'node:child_process';

export const PLANS = join(dirname(fileURLToPath(import.meta.url)), '..');
export const ROOT = join(PLANS, '..');
export const REPO = 'terminal-grid';

// Every checkout of this repo: this one plus its git worktrees. Epics are
// built on their own branch (epic/<id>) in their own worktree, so the tools
// look there for an epic's cards, status and activity while it is building.
export function checkouts() {
  const out = [{ path: ROOT, branch: currentBranch(ROOT), main: true }];
  try {
    const text = execFileSync('git', ['-C', ROOT, 'worktree', 'list', '--porcelain'], {
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'ignore'],
    });
    for (const block of text.trim().split(/\n\n+/)) {
      const path = block.match(/^worktree (.+)$/m)?.[1];
      const branch = block.match(/^branch refs\/heads\/(.+)$/m)?.[1] ?? '';
      if (path && resolve(path) !== resolve(ROOT)) out.push({ path, branch, main: false });
    }
  } catch {
    // Not a git repo, or git is missing: only this checkout.
  }
  return out;
}

function currentBranch(dir) {
  try {
    return execFileSync('git', ['-C', dir, 'branch', '--show-current'], {
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'ignore'],
    }).trim();
  } catch {
    return '';
  }
}

// The checkout an epic is read from: the worktree on its epic or dry run
// branch when one exists and has the epic folder, otherwise this checkout.
export function epicHome(epic) {
  const all = checkouts();
  const own = all.find(
    (c) => !c.main && (c.branch === `epic/${epic}` || c.branch === `dryrun/${epic}`) && existsSync(join(c.path, 'plans', epic, 'epic.md')),
  );
  return own ?? all[0];
}

export function parseFrontMatter(text) {
  const m = text.match(/^---\r?\n([\s\S]*?)\r?\n---\r?\n?/);
  if (!m) return { data: null, body: text };
  const data = {};
  let listKey = null;
  for (const raw of m[1].split(/\r?\n/)) {
    const line = raw.replace(/\s+#.*$/, '');
    if (!line.trim()) continue;
    const item = line.match(/^\s+-\s+(.*)$/);
    if (item && listKey) {
      data[listKey].push(unquote(item[1].trim()));
      continue;
    }
    const kv = line.match(/^([A-Za-z_][\w-]*):\s*(.*)$/);
    if (!kv) continue;
    const [, key, value] = kv;
    if (value === '') {
      data[key] = [];
      listKey = key;
    } else if (value.startsWith('[')) {
      data[key] = value
        .replace(/^\[|\]$/g, '')
        .split(',')
        .map((v) => unquote(v.trim()))
        .filter(Boolean);
      listKey = null;
    } else {
      data[key] = unquote(value.trim());
      listKey = null;
    }
  }
  return { data, body: text.slice(m[0].length) };
}

function unquote(v) {
  return v.replace(/^["'](.*)["']$/, '$1');
}

// Epic folders across every checkout, so an epic started in a worktree and
// not yet merged is listed too.
export function listEpics() {
  const ids = new Set();
  for (const c of checkouts()) {
    const plans = join(c.path, 'plans');
    if (!existsSync(plans)) continue;
    for (const d of readdirSync(plans, { withFileTypes: true })) {
      if (d.isDirectory() && /^\d{3}-/.test(d.name)) ids.add(d.name);
    }
  }
  return [...ids].sort();
}

export function loadEpic(epic) {
  const checkout = epicHome(epic);
  const dir = join(checkout.path, 'plans', epic);
  const epicFile = join(dir, 'epic.md');
  const meta = existsSync(epicFile)
    ? parseFrontMatter(readFileSync(epicFile, 'utf8')).data
    : null;
  const tasksDir = join(dir, 'tasks');
  const cards = existsSync(tasksDir)
    ? readdirSync(tasksDir)
        .filter((f) => /^T\d+.*\.md$/.test(f))
        .sort()
        .map((f) => {
          const path = join(tasksDir, f);
          const text = readFileSync(path, 'utf8');
          const { data, body } = parseFrontMatter(text);
          return { file: f, path, data: data ?? {}, body, text };
        })
    : [];
  return { epic, dir, meta, cards, checkout };
}
