// Shared helpers for the plan tools. No dependencies: the front matter used in
// plans/ is a small subset of YAML (scalars, inline [] lists, block "- x"
// lists, and "# comments"), parsed here by hand.
import { readFileSync, readdirSync, existsSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

export const PLANS = join(dirname(fileURLToPath(import.meta.url)), '..');
export const ROOT = join(PLANS, '..');

// Single repo: everything builds here. Kept as functions so the tools stay
// simple to extend if a second repo is ever added.
export function repos() {
  return { 'terminal-grid': ROOT };
}

export function cardRepo() {
  return 'terminal-grid';
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

export function listEpics() {
  return readdirSync(PLANS, { withFileTypes: true })
    .filter((d) => d.isDirectory() && /^\d{3}-/.test(d.name))
    .map((d) => d.name)
    .sort();
}

export function loadEpic(epic) {
  const dir = join(PLANS, epic);
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
  return { epic, dir, meta, cards };
}
