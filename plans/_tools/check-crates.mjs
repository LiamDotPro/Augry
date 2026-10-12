#!/usr/bin/env node
// Keeps trigger crates simple and isolated. Checks every crate in
// src-tauri/crates/ against the rules in docs/process-triggers.md Part 2:
//
// - It has a README.md with every required section and the kinds markers.
// - Its path dependencies are trigger-core only (trigger-core itself has none),
//   and it never depends on tauri or the app.
// - Its files stay small: a file over 400 lines is a warning, over 800 an error.
// - It has the standard files: src/lib.rs, src/rule.rs, src/logic.rs, src/os.rs,
//   tests/readme.rs, tests/live.rs (trigger-core and trigger-hub excepted).
//
//   node plans/_tools/check-crates.mjs [--dir <crates dir>]
import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs';
import { join, relative } from 'node:path';
import { ROOT } from './cards.mjs';

const argv = process.argv.slice(2);
const dirFlag = argv.indexOf('--dir');
const CRATES = dirFlag >= 0 ? argv[dirFlag + 1] : join(ROOT, 'src-tauri', 'crates');

const SECTIONS = ['Kinds', 'Rule fields', 'Payload', 'Platforms', 'Permissions', 'Example', 'Develop', 'Known limits'];
const CORE_SECTIONS = ['Develop'];
const FILES = ['src/lib.rs', 'src/rule.rs', 'src/logic.rs', 'src/os.rs', 'tests/readme.rs', 'tests/live.rs'];
const INFRA = new Set(['trigger-core', 'trigger-hub']);

let errors = 0;
let warnings = 0;
const err = (c, m) => (errors++, console.log(`ERROR ${c}: ${m}`));
const warn = (c, m) => (warnings++, console.log(`warn  ${c}: ${m}`));

if (!existsSync(CRATES)) {
  console.log('No crates yet (src-tauri/crates/ does not exist).\n\n0 error(s), 0 warning(s)');
  process.exit(0);
}

const crates = readdirSync(CRATES).filter((d) => statSync(join(CRATES, d)).isDirectory());
for (const name of crates) {
  const dir = join(CRATES, name);
  if (!name.startsWith('trigger-')) {
    err(name, 'crates in src-tauri/crates/ are trigger crates and are named trigger-<id>');
    continue;
  }
  const manifestPath = join(dir, 'Cargo.toml');
  if (!existsSync(manifestPath)) {
    err(name, 'no Cargo.toml');
    continue;
  }
  const manifest = readFileSync(manifestPath, 'utf8');

  // Dependencies: every path dependency, and tauri by name.
  const pathDeps = [...manifest.matchAll(/^([\w-]+)\s*=\s*\{[^}]*\bpath\s*=\s*"([^"]+)"/gm)].map((m) => m[1]);
  const allowed = name === 'trigger-core' ? [] : ['trigger-core'];
  for (const dep of pathDeps) {
    if (!allowed.includes(dep)) err(name, `depends on ${dep}; a trigger crate may only depend on ${allowed.join(', ') || 'outside crates'}`);
  }
  if (/^\s*(tauri|tauri-[\w-]+|terminal-grid)\s*=/m.test(manifest)) err(name, 'depends on tauri or the app; trigger crates know nothing about the app');
  if (!/^publish\s*=\s*false/m.test(manifest)) warn(name, 'Cargo.toml should set publish = false');

  // README.
  const readmePath = join(dir, 'README.md');
  if (!existsSync(readmePath)) {
    err(name, 'no README.md');
  } else {
    const readme = readFileSync(readmePath, 'utf8');
    for (const s of INFRA.has(name) ? CORE_SECTIONS : SECTIONS) {
      if (!new RegExp(`^## ${s}\\s*$`, 'm').test(readme)) err(name, `README.md has no "## ${s}" section`);
    }
    if (!INFRA.has(name) && !(readme.includes('<!-- kinds -->') && readme.includes('<!-- /kinds -->'))) {
      err(name, 'README.md has no <!-- kinds --> ... <!-- /kinds --> markers');
    }
    if (/Replace with|not implemented yet/.test(readme)) warn(name, 'README.md still has template text');
  }
  for (const f of ['src/lib.rs', 'src/logic.rs', 'src/os.rs', 'src/rule.rs']) {
    const p = join(dir, f);
    if (existsSync(p) && /Template: remove|not implemented yet|\{\{\w+\}\}/.test(readFileSync(p, 'utf8'))) warn(name, `${f} still has template code`);
  }

  // Standard files.
  if (!INFRA.has(name)) {
    for (const f of FILES) if (!existsSync(join(dir, f))) err(name, `missing ${f} (see plans/_templates/trigger-crate)`);
  }

  // Size.
  const walk = (d) =>
    readdirSync(d).flatMap((e) => {
      const p = join(d, e);
      return statSync(p).isDirectory() ? (e === 'target' ? [] : walk(p)) : p.endsWith('.rs') ? [p] : [];
    });
  for (const file of walk(dir)) {
    const lines = readFileSync(file, 'utf8').split('\n').length;
    const rel = relative(dir, file);
    if (lines > 800) err(name, `${rel} has ${lines} lines; split it`);
    else if (lines > 400) warn(name, `${rel} has ${lines} lines; consider splitting it`);
  }
}

console.log(`\n${crates.length} crate(s), ${errors} error(s), ${warnings} warning(s)`);
process.exit(errors ? 1 : 0);
