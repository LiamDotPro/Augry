---
id: T23
epic: 010-trigger-docker
title: Plan which engine serves which rules
status: done
depends_on: [T22]
size: S
model: haiku
files:
  - src-tauri/crates/trigger-docker/src/lib.rs
  - src-tauri/crates/trigger-docker/src/plan.rs
  - src-tauri/crates/trigger-docker/src/plan/tests.rs
---

## Goal

`src/plan.rs` has the pure `plan` function that turns the armed rules into groups, one per engine that will run: an explicit `docker` or `podman` rule gets that engine, `auto` gets docker if it answers and podman otherwise, and anything unusable gives a message for Settings. The check that an engine answers is passed in, so ten unit tests need no engine.

## Read first

- `plans/010-trigger-docker/epic.md` Decision 8 (engine choice, the exact message, one child per engine)
- `src-tauri/crates/trigger-docker/src/os.rs` the `Probe` enum (`Missing`, `Silent(path)`, `Ready(path)`)
- `src-tauri/crates/trigger-docker/src/rule.rs:10-18` `EngineChoice`; `:35-41` `Parsed` (its `when.engine` is the choice)
- `src-tauri/crates/trigger-docker/src/logic.rs:15-22` `Armed` and `:26-30` `Engine`
- `src-tauri/crates/trigger-docker/src/lib.rs:1-12` the `mod` lines

## Steps

1. In `src-tauri/crates/trigger-docker/src/lib.rs` find the line `mod os;` and add a line directly under it so the top reads:

```rust
mod logic;
mod os;
mod plan;
mod rule;
mod supervisor;
```

2. Run `mkdir -p src-tauri/crates/trigger-docker/src/plan`, then create `src-tauri/crates/trigger-docker/src/plan.rs` with exactly this content. When any rule cannot get an engine the whole plan fails with that message, so a rule for an engine that is not running makes the source unavailable in Settings rather than silently doing less.

```rust
//! Decides which engine runs which rules. Pure: the check that an engine
//! answers is passed in, so every case is a unit test.

// Removed when lib.rs reads everything in this file (slice C).
#![allow(dead_code)]

use std::path::PathBuf;

use crate::logic::{Armed, Engine};
use crate::os::Probe;
use crate::rule::EngineChoice;

/// The rules one engine child will serve.
#[derive(Debug)]
pub struct Group {
    pub engine: Engine,
    pub path: PathBuf,
    pub rules: Vec<Armed>,
}

/// Asks `probe` about each engine at most once.
struct Probes<'a> {
    probe: &'a dyn Fn(Engine) -> Probe,
    seen: Vec<(Engine, Probe)>,
}

impl Probes<'_> {
    fn get(&mut self, engine: Engine) -> Probe {
        if let Some((_, known)) = self.seen.iter().find(|(e, _)| *e == engine) {
            return known.clone();
        }
        let found = (self.probe)(engine);
        self.seen.push((engine, found.clone()));
        found
    }
}

/// What the Settings page shows when the engine is there but silent.
fn silent_message(engine: Engine, path: &std::path::Path) -> String {
    format!(
        "{} found at {} but the engine did not answer",
        engine.as_str(),
        path.display()
    )
}

/// Why one named engine cannot be used.
fn explain(engine: Engine, probe: &Probe) -> String {
    match probe {
        Probe::Silent(path) => silent_message(engine, path),
        _ => format!("{} was not found", engine.as_str()),
    }
}

/// Groups the rules by the engine they will use. `auto` takes docker if it
/// answers, else podman. Any rule that cannot get an engine fails the whole
/// plan with the reason, so Settings can show it.
pub fn plan(armed: Vec<Armed>, probe: &dyn Fn(Engine) -> Probe) -> Result<Vec<Group>, String> {
    let mut probes = Probes {
        probe,
        seen: Vec::new(),
    };
    let mut groups: Vec<Group> = Vec::new();
    for rule in armed {
        let (engine, path) = match rule.when.when.engine {
            EngineChoice::Docker => ready(&mut probes, Engine::Docker)?,
            EngineChoice::Podman => ready(&mut probes, Engine::Podman)?,
            EngineChoice::Auto => ready_auto(&mut probes)?,
        };
        match groups.iter_mut().find(|g| g.engine == engine) {
            Some(group) => group.rules.push(rule),
            None => groups.push(Group {
                engine,
                path,
                rules: vec![rule],
            }),
        }
    }
    Ok(groups)
}

fn ready(probes: &mut Probes, engine: Engine) -> Result<(Engine, PathBuf), String> {
    match probes.get(engine) {
        Probe::Ready(path) => Ok((engine, path)),
        other => Err(explain(engine, &other)),
    }
}

fn ready_auto(probes: &mut Probes) -> Result<(Engine, PathBuf), String> {
    let mut reasons = Vec::new();
    for engine in [Engine::Docker, Engine::Podman] {
        match probes.get(engine) {
            Probe::Ready(path) => return Ok((engine, path)),
            Probe::Silent(path) => reasons.push(silent_message(engine, &path)),
            Probe::Missing => {}
        }
    }
    if reasons.is_empty() {
        Err("docker and podman were not found".to_string())
    } else {
        Err(reasons.join("; "))
    }
}

#[cfg(test)]
mod tests;
```

3. Create `src-tauri/crates/trigger-docker/src/plan/tests.rs` with exactly this content.

```rust
use std::cell::Cell;

use serde_json::json;
use trigger_core::testkit::rule;

use super::*;

fn armed(id: &str, engine: &str) -> Armed {
    let r = rule(id, "docker.container-started", json!({ "engine": engine }));
    let when = crate::rule::parse(&r).unwrap();
    Armed {
        id: r.id,
        kind: r.kind,
        when,
    }
}

fn ready_at(path: &str) -> Probe {
    Probe::Ready(PathBuf::from(path))
}

fn silent_at(path: &str) -> Probe {
    Probe::Silent(PathBuf::from(path))
}

fn ids(group: &Group) -> Vec<&str> {
    group.rules.iter().map(|r| r.id.as_str()).collect()
}

#[test]
fn an_explicit_engine_gets_its_own_group() {
    let probe = |e: Engine| match e {
        Engine::Docker => ready_at("/bin/docker"),
        Engine::Podman => Probe::Missing,
    };
    let groups = plan(vec![armed("p/a", "docker")], &probe).unwrap();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].engine, Engine::Docker);
    assert_eq!(groups[0].path, PathBuf::from("/bin/docker"));
    assert_eq!(ids(&groups[0]), vec!["p/a"]);
}

#[test]
fn auto_takes_docker_and_never_asks_podman() {
    let asked = Cell::new(0);
    let probe = |e: Engine| {
        asked.set(asked.get() + 1);
        match e {
            Engine::Docker => ready_at("/bin/docker"),
            Engine::Podman => ready_at("/bin/podman"),
        }
    };
    let groups = plan(vec![armed("p/a", "auto")], &probe).unwrap();
    assert_eq!(groups[0].engine, Engine::Docker);
    assert_eq!(asked.get(), 1);
}

#[test]
fn auto_falls_back_to_podman() {
    let probe = |e: Engine| match e {
        Engine::Docker => silent_at("/bin/docker"),
        Engine::Podman => ready_at("/bin/podman"),
    };
    let groups = plan(vec![armed("p/a", "auto")], &probe).unwrap();
    assert_eq!(groups[0].engine, Engine::Podman);
    assert_eq!(groups[0].path, PathBuf::from("/bin/podman"));
}

#[test]
fn auto_with_no_engine_says_neither_was_found() {
    let probe = |_: Engine| Probe::Missing;
    let err = plan(vec![armed("p/a", "auto")], &probe).unwrap_err();
    assert_eq!(err, "docker and podman were not found");
}

#[test]
fn a_silent_engine_is_reported_with_its_path() {
    let probe = |e: Engine| match e {
        Engine::Docker => silent_at("/usr/local/bin/docker"),
        Engine::Podman => Probe::Missing,
    };
    let err = plan(vec![armed("p/a", "auto")], &probe).unwrap_err();
    assert_eq!(
        err,
        "docker found at /usr/local/bin/docker but the engine did not answer"
    );
    let err = plan(vec![armed("p/a", "docker")], &probe).unwrap_err();
    assert_eq!(
        err,
        "docker found at /usr/local/bin/docker but the engine did not answer"
    );
}

#[test]
fn two_silent_engines_are_both_reported() {
    let probe = |e: Engine| match e {
        Engine::Docker => silent_at("/d/docker"),
        Engine::Podman => silent_at("/p/podman"),
    };
    let err = plan(vec![armed("p/a", "auto")], &probe).unwrap_err();
    assert_eq!(
        err,
        "docker found at /d/docker but the engine did not answer; podman found at /p/podman but the engine did not answer"
    );
}

#[test]
fn an_explicit_engine_that_is_missing_fails_even_if_the_other_works() {
    let probe = |e: Engine| match e {
        Engine::Docker => ready_at("/bin/docker"),
        Engine::Podman => Probe::Missing,
    };
    let err = plan(vec![armed("p/a", "podman")], &probe).unwrap_err();
    assert_eq!(err, "podman was not found");
}

#[test]
fn rules_naming_different_engines_make_one_group_each() {
    let probe = |e: Engine| match e {
        Engine::Docker => ready_at("/bin/docker"),
        Engine::Podman => ready_at("/bin/podman"),
    };
    let rules = vec![
        armed("p/a", "podman"),
        armed("p/b", "docker"),
        armed("p/c", "auto"),
    ];
    let groups = plan(rules, &probe).unwrap();
    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0].engine, Engine::Podman);
    assert_eq!(ids(&groups[0]), vec!["p/a"]);
    assert_eq!(groups[1].engine, Engine::Docker);
    assert_eq!(ids(&groups[1]), vec!["p/b", "p/c"]);
}

#[test]
fn each_engine_is_asked_once() {
    let asked = Cell::new(0);
    let probe = |_: Engine| {
        asked.set(asked.get() + 1);
        ready_at("/bin/engine")
    };
    let rules = vec![
        armed("p/a", "docker"),
        armed("p/b", "auto"),
        armed("p/c", "docker"),
    ];
    let groups = plan(rules, &probe).unwrap();
    assert_eq!(groups.len(), 1);
    assert_eq!(ids(&groups[0]), vec!["p/a", "p/b", "p/c"]);
    assert_eq!(asked.get(), 1);
}

#[test]
fn no_rules_means_no_groups_and_no_questions() {
    let probe = |_: Engine| -> Probe { panic!("must not be asked") };
    assert!(plan(Vec::new(), &probe).unwrap().is_empty());
}
```

4. Run `cd src-tauri && cargo fmt -p trigger-docker`, then the acceptance commands.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-docker plan::` prints `10 passed` for the library
- [ ] `cd src-tauri && cargo test -p trigger-docker` passes
- [ ] `cd src-tauri && cargo clippy -p trigger-docker --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-docker --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0 (it may print warnings for template text in `README.md` and `src/lib.rs`; T24 and T27 remove them)
- [ ] `git status --porcelain` lists only paths under `files:`, ignoring other card files under `plans/010-trigger-docker/tasks/`

## Out of scope

- Calling `plan` from `lib.rs` (T24) or running a program from it. It stays pure.
- Starting some engines when others fail. One failure fails the whole plan; that is the decision.
- Changing the messages. `docker found at <path> but the engine did not answer` is fixed by Decision 8, and `docker and podman were not found` and `<engine> was not found` follow its wording.
- Removing the `#![allow(dead_code)]` line in `plan.rs`. T24 removes it.
- The README. Internal code only; the card that writes the README is T27, which describes this work.

## If stuck

- If `crate::os::Probe` does not exist, T21 did not run. Set status blocked.
- If `lib.rs` has no line `mod os;`, set status blocked and say what the top of the file looks like.
- If `cargo fmt -p trigger-docker --check` fails, run `cd src-tauri && cargo fmt -p trigger-docker` once and carry on.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed
- 2026-10-12 checker: pass
