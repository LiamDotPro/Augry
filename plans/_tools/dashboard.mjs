#!/usr/bin/env node
// Live progress dashboard for the planning pipeline. No dependencies.
//
//   node plans/_tools/dashboard.mjs [--port 4317] [--open]
//
// Serves a page on http://localhost:4317 that shows every epic and card, the
// card being worked on right now, the activity feed from
// plans/_runs/activity.jsonl, and recent commits. Epics are built in their
// own worktrees (one per epic branch), so the page reads every checkout of
// this repo: an epic comes from the worktree on its branch while it builds,
// the activity feeds are merged, and each checkout gets a commit list. The
// page updates itself when files change.
import { createServer } from 'node:http';
import { readFileSync, existsSync, watch, statSync } from 'node:fs';
import { join, basename } from 'node:path';
import { execFileSync, spawn } from 'node:child_process';
import { PLANS, REPO, listEpics, loadEpic, checkouts } from './cards.mjs';

const argv = process.argv.slice(2);
const port = Number(argv[argv.indexOf('--port') + 1]) || Number(process.env.PORT) || 4317;
const HTML = join(PLANS, '_tools', 'dashboard.html');

function section(body, name) {
  const m = body.match(new RegExp(`^## ${name}\\s*$([\\s\\S]*?)(?=^## |$(?![\\s\\S]))`, 'm'));
  return m ? m[1].trim() : '';
}

function checkoutLabel(c) {
  return c.main ? REPO : basename(c.path);
}

// The activity of every checkout, merged in time order. A skill logs to the
// plans/_runs/ of the checkout it runs in.
function readActivity(all, limit = 400) {
  const lines = [];
  for (const c of all) {
    const file = join(c.path, 'plans', '_runs', 'activity.jsonl');
    if (!existsSync(file)) continue;
    for (const l of readFileSync(file, 'utf8').trim().split('\n').filter(Boolean)) {
      try {
        lines.push({ ...JSON.parse(l), checkout: checkoutLabel(c) });
      } catch {
        // A half written line; skip it.
      }
    }
  }
  lines.sort((a, b) => String(a.ts).localeCompare(String(b.ts)));
  return lines.slice(-limit);
}

function gitLog(dir) {
  try {
    const out = execFileSync('git', ['-C', dir, 'log', '-n', '12', '--format=%h%x1f%cI%x1f%s'], {
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'ignore'],
    });
    const branch = execFileSync('git', ['-C', dir, 'branch', '--show-current'], {
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'ignore'],
    }).trim();
    const dirty = execFileSync('git', ['-C', dir, 'status', '--porcelain'], {
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'ignore'],
    })
      .split('\n')
      .filter(Boolean).length;
    return {
      branch,
      dirty,
      commits: out
        .trim()
        .split('\n')
        .filter(Boolean)
        .map((l) => {
          const [hash, date, subject] = l.split('\x1f');
          return { hash, date, subject };
        }),
    };
  } catch {
    return null;
  }
}

function state() {
  const all = checkouts();
  const epics = listEpics().map((id) => {
    const { meta, cards, checkout, dir } = loadEpic(id);
    const research = (() => {
      try {
        return readFileSync(join(dir, 'epic.md'), 'utf8').match(/^\| R\d\d \|/gm)?.length ?? 0;
      } catch {
        return 0;
      }
    })();
    return {
      id,
      title: meta?.title ?? id,
      status: meta?.status ?? 'unknown',
      worktree: checkout.main ? null : checkout.branch,
      dependsOn: meta?.depends_on ?? [],
      research,
      cards: cards.map((c) => ({
        id: c.data.id,
        file: c.file,
        title: c.data.title,
        status: c.data.status,
        size: c.data.size,
        model: c.data.model ?? 'haiku',
        dependsOn: c.data.depends_on ?? [],
        files: c.data.files ?? [],
        goal: section(c.body, 'Goal'),
        log: section(c.body, 'Log')
          .split('\n')
          .filter((l) => l.startsWith('- '))
          .map((l) => l.slice(2)),
      })),
    };
  });
  const git = Object.fromEntries(all.map((c) => [checkoutLabel(c), gitLog(c.path)]));
  return { generatedAt: new Date().toISOString(), epics, activity: readActivity(all), git };
}

// Server-sent events: tell open pages to refetch when plans or git change.
const clients = new Set();
let timer = null;
function changed() {
  clearTimeout(timer);
  timer = setTimeout(() => {
    for (const res of clients) res.write('data: changed\n\n');
  }, 250);
}
const watched = new Set();
function watchPath(path, recursive) {
  if (watched.has(path) || !existsSync(path)) return;
  watched.add(path);
  try {
    watch(path, { recursive }, changed);
  } catch {
    // Recursive watch is not available everywhere; the page also polls.
  }
}
// Watches every checkout's plans/ and the shared .git (worktree refs live
// under .git/worktrees/). New worktrees are picked up on the next scan.
let known = '';
function watchCheckouts() {
  const all = checkouts();
  for (const c of all) {
    watchPath(join(c.path, 'plans'), true);
    const gitDir = join(c.path, '.git');
    if (existsSync(gitDir) && statSync(gitDir).isDirectory()) {
      watchPath(gitDir, false);
      watchPath(join(gitDir, 'worktrees'), true);
      watchPath(join(gitDir, 'refs', 'heads'), true);
    }
  }
  const now = all.map((c) => c.path).join('\n');
  if (known && now !== known) changed();
  known = now;
}
watchCheckouts();
setInterval(watchCheckouts, 15000).unref();

const server = createServer((req, res) => {
  if (req.url === '/' || req.url === '/index.html') {
    res.writeHead(200, { 'content-type': 'text/html; charset=utf-8', 'cache-control': 'no-store' });
    res.end(readFileSync(HTML));
  } else if (req.url === '/api/state') {
    res.writeHead(200, { 'content-type': 'application/json', 'cache-control': 'no-store' });
    res.end(JSON.stringify(state()));
  } else if (req.url === '/events') {
    res.writeHead(200, {
      'content-type': 'text/event-stream',
      'cache-control': 'no-store',
      connection: 'keep-alive',
    });
    res.write('retry: 2000\n\n');
    clients.add(res);
    const ping = setInterval(() => res.write(': ping\n\n'), 20000);
    req.on('close', () => {
      clearInterval(ping);
      clients.delete(res);
    });
  } else {
    res.writeHead(404).end('not found');
  }
});

server.listen(port, '127.0.0.1', () => {
  const url = `http://localhost:${port}`;
  console.log(`Plan dashboard on ${url}  (Ctrl+C to stop)`);
  if (argv.includes('--open')) {
    const cmd = process.platform === 'darwin' ? 'open' : process.platform === 'win32' ? 'cmd' : 'xdg-open';
    const args = process.platform === 'win32' ? ['/c', 'start', '', url] : [url];
    spawn(cmd, args, { stdio: 'ignore', detached: true }).unref();
  }
});
