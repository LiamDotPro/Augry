#!/usr/bin/env node
// Creates a new trigger crate from plans/_templates/trigger-crate/, so every
// crate starts with the same files, README sections and tests.
//
//   node plans/_tools/new-trigger-crate.mjs <name> "<one sentence summary>" [--dest <crates dir>]
//
// <name> is the source id: lower case letters, digits and hyphens, such as
// "git" or "docker". The crate is src-tauri/crates/trigger-<name>.
import { cpSync, existsSync, readdirSync, readFileSync, statSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { execFileSync } from 'node:child_process';
import { PLANS, ROOT } from './cards.mjs';

const argv = process.argv.slice(2);
const destFlag = argv.indexOf('--dest');
const dest = destFlag >= 0 ? argv.splice(destFlag, 2)[1] : join(ROOT, 'src-tauri', 'crates');
const [name, summary] = argv;
if (!name || !summary || !/^[a-z]([a-z0-9-]*[a-z0-9])?$/.test(name)) {
  console.error('usage: new-trigger-crate.mjs <name> "<one sentence summary>" [--dest <crates dir>]');
  console.error('       <name>: lower case letters, digits and hyphens, for example git or docker');
  process.exit(2);
}

const target = join(dest, `trigger-${name}`);
if (existsSync(target)) {
  console.error(`${target} already exists`);
  process.exit(1);
}

const values = {
  name,
  crate: name.replace(/-/g, '_'),
  Name: name.split('-').map((p) => p[0].toUpperCase() + p.slice(1)).join(''),
  summary: summary.replace(/\.?$/, '.'),
};

cpSync(join(PLANS, '_templates', 'trigger-crate'), target, { recursive: true });
const fill = (dir) => {
  for (const entry of readdirSync(dir)) {
    const path = join(dir, entry);
    if (statSync(path).isDirectory()) fill(path);
    else writeFileSync(path, readFileSync(path, 'utf8').replace(/\{\{(\w+)\}\}/g, (m, k) => values[k] ?? m));
  }
};
fill(target);

// Line breaks depend on the crate name's length, so format what was written.
const rustFiles = [];
const collect = (dir) => {
  for (const entry of readdirSync(dir)) {
    const path = join(dir, entry);
    if (statSync(path).isDirectory()) collect(path);
    else if (path.endsWith('.rs')) rustFiles.push(path);
  }
};
collect(target);
try {
  execFileSync('rustfmt', ['--edition', '2021', ...rustFiles], { stdio: 'ignore' });
} catch {
  console.warn('rustfmt not found or failed: run cargo fmt before committing.');
}

console.log(`Created ${target}

Next:
  1. Replace the example kind in src/lib.rs, the When fields in src/rule.rs,
     Signal and step in src/logic.rs, and the OS code in src/os.rs.
  2. Fill in every README section. Refresh its Kinds table with
     cargo run -p trigger-${name} --example readme
  3. Register it in the app: in src-tauri/Cargo.toml add the optional
     dependency and the feature trigger-${name}, and one line in sources()
     in src-tauri/src/triggers/mod.rs.
  4. node plans/_tools/check-crates.mjs`);
