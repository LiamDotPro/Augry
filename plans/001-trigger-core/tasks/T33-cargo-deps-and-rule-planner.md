---
id: T33
epic: 001-trigger-core
title: Add the app dependencies on the crates and the rule planner
status: done
depends_on: [T32, T17, T29]
size: M
model: haiku
files:
  - src-tauri/Cargo.toml
  - src-tauri/Cargo.lock
  - src-tauri/src/triggers/mod.rs
  - src-tauri/src/triggers/rules.rs
---

## Goal

The app crate depends on `trigger-core`, `trigger-hub` and (behind the default cargo feature `trigger-fs`) `trigger-fs`, and `src-tauri/src/triggers/rules.rs` turns local definitions, the armed list and the open repositories into the rules the hub runs. The function `plan` is pure and tested. Nothing calls it yet.

## Read first

- `plans/001-trigger-core/epic.md` Decision 12 (rule ids and `${repo.root}` fan out), Decision 13 and the "Workspace" block of the Shape
- `src-tauri/Cargo.toml` the file you edit
- `src-tauri/src/triggers/store.rs` `LocalDefinition` and `DefinitionTrigger` (T30)
- `src-tauri/src/triggers/variables.rs` `Context`, `expand` and `Expanded` (T32)
- `src-tauri/crates/trigger-core/src/lib.rs` `Rule` and `RuleId`

## Steps

1. Before changing anything, run `cd src-tauri && cargo clippy -p terminal-grid --all-targets 2>&1 | grep -c warning` and write the number in the Log. It is the baseline for the clippy check in Acceptance.

2. In `src-tauri/Cargo.toml` add the cargo feature block. Insert this block directly above the line `[dependencies]`, with one blank line before and after it:

```toml
[features]
default = ["trigger-fs"]
trigger-fs = ["dep:trigger-fs"]
```

3. In the same file add three lines directly under the line `[dependencies]` (before `tauri = ...`):

```toml
trigger-core = { path = "crates/trigger-core" }
trigger-hub = { path = "crates/trigger-hub" }
trigger-fs = { path = "crates/trigger-fs", optional = true }
```

Change nothing else in the file.

4. In `src-tauri/src/triggers/mod.rs` add this line directly above `pub mod store;`, so the `pub mod` lines stay in alphabetical order:

```rust
pub mod rules;
```

5. Create `src-tauri/src/triggers/rules.rs` with exactly this content. `plan` gives each armed process one `PlannedRule` per trigger (or per repository root when `${repo.root}` is used) and a `Rule` for the hub for every one that has values. `process_of` takes the process id out of a rule id.

```rust
//! Turns local definitions into the rules the hub runs. Pure: no files, no
//! threads. Rule ids are `<processId>/<triggerId>`, with `#<n>` added when
//! `${repo.root}` fans a trigger out over several repositories (n starts at 1).

use trigger_core::{Rule, RuleId};

use super::store::LocalDefinition;
use super::variables::{expand, Context, Expanded};
use crate::state::TriggersConfig;

/// One trigger of one process as the Settings list shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct PlannedRule {
    pub id: RuleId,
    pub kind: String,
    /// Why the rule is not given to the hub; None when it is.
    pub skipped: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlannedProcess {
    pub id: String,
    pub name: String,
    pub armed: bool,
    pub problem: Option<String>,
    /// Empty unless the process is armed and has no problem.
    pub rules: Vec<PlannedRule>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Plan {
    /// What `Hub::apply` gets, sorted by id so two plans compare equal.
    pub rules: Vec<Rule>,
    pub processes: Vec<PlannedProcess>,
}

/// With triggers off the plan is empty: no rules and no processes.
pub fn plan(defs: &[LocalDefinition], config: &TriggersConfig, ctx: &Context) -> Plan {
    if !config.enabled {
        return Plan::default();
    }
    let mut plan = Plan::default();
    for def in defs {
        let armed = config.armed.contains(&def.id);
        let mut process = PlannedProcess {
            id: def.id.clone(),
            name: def.name.clone(),
            armed,
            problem: def.problem.clone(),
            rules: Vec::new(),
        };
        if armed && def.problem.is_none() {
            for trigger in &def.triggers {
                let base = format!("{}/{}", def.id, trigger.id);
                match expand(&trigger.when, ctx) {
                    Expanded::Rules(expanded) => {
                        for (suffix, when) in expanded {
                            let id = format!("{base}{suffix}");
                            process.rules.push(PlannedRule {
                                id: id.clone(),
                                kind: trigger.kind.clone(),
                                skipped: None,
                            });
                            plan.rules.push(Rule {
                                id,
                                kind: trigger.kind.clone(),
                                when,
                            });
                        }
                    }
                    Expanded::Skipped(reason) => process.rules.push(PlannedRule {
                        id: base,
                        kind: trigger.kind.clone(),
                        skipped: Some(reason),
                    }),
                }
            }
        }
        plan.processes.push(process);
    }
    plan.rules.sort_by(|a, b| a.id.cmp(&b.id));
    plan
}

/// The process id a rule id belongs to: everything before the first `/`.
pub fn process_of(rule_id: &str) -> &str {
    rule_id
        .split_once('/')
        .map_or(rule_id, |(process, _)| process)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::triggers::store::{parse, LocalDefinition};
    use std::path::PathBuf;

    fn ctx(roots: &[&str]) -> Context {
        Context {
            home: PathBuf::from("/home/me"),
            downloads: None,
            notes_root: PathBuf::from("/notes"),
            repo_roots: roots.iter().map(|r| r.to_string()).collect(),
            screenshots: || None,
        }
    }

    fn config(enabled: bool, armed: &[&str]) -> TriggersConfig {
        TriggersConfig {
            enabled,
            armed: armed.iter().map(|a| a.to_string()).collect(),
        }
    }

    fn tidy() -> LocalDefinition {
        parse(
            "tidy",
            r#"{"schemaVersion":1,"name":"Keep the docs tidy","triggers":[
                {"id":"saved","kind":"fs.file-saved","when":{"path":"${repo.root}/docs"}},
                {"id":"dl","kind":"fs.download-finished","when":{"path":"${downloads}"}}
            ]}"#,
        )
    }

    #[test]
    fn triggers_off_plans_nothing() {
        let plan = plan(&[tidy()], &config(false, &["tidy"]), &ctx(&["/r"]));
        assert_eq!(plan, Plan::default());
    }

    #[test]
    fn an_unarmed_process_is_listed_without_rules() {
        let plan = plan(&[tidy()], &config(true, &[]), &ctx(&["/r"]));
        assert!(plan.rules.is_empty());
        assert_eq!(plan.processes.len(), 1);
        assert!(!plan.processes[0].armed);
        assert!(plan.processes[0].rules.is_empty());
    }

    #[test]
    fn an_armed_process_gets_one_rule_per_repository() {
        let plan = plan(&[tidy()], &config(true, &["tidy"]), &ctx(&["/r/b", "/r/a"]));
        let ids: Vec<&str> = plan.rules.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, ["tidy/saved#1", "tidy/saved#2"]);
        assert_eq!(plan.rules[0].kind, "fs.file-saved");
        assert_eq!(plan.rules[0].when["path"], serde_json::json!("/r/a/docs"));
    }

    #[test]
    fn a_rule_with_no_value_is_listed_as_skipped() {
        let plan = plan(&[tidy()], &config(true, &["tidy"]), &ctx(&["/r"]));
        let rules = &plan.processes[0].rules;
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].id, "tidy/saved#1");
        assert_eq!(rules[0].skipped, None);
        assert_eq!(rules[1].id, "tidy/dl");
        assert!(rules[1].skipped.as_deref().unwrap().contains("downloads"));
        assert_eq!(plan.rules.len(), 1);
    }

    #[test]
    fn a_definition_with_a_problem_gets_no_rules_even_when_armed() {
        let broken = parse("bad", "{");
        let plan = plan(&[broken], &config(true, &["bad"]), &ctx(&[]));
        assert!(plan.rules.is_empty());
        assert!(plan.processes[0].problem.is_some());
        assert!(plan.processes[0].rules.is_empty());
    }

    #[test]
    fn the_same_inputs_give_equal_plans() {
        let one = plan(&[tidy()], &config(true, &["tidy"]), &ctx(&["/r/a", "/r/b"]));
        let two = plan(&[tidy()], &config(true, &["tidy"]), &ctx(&["/r/b", "/r/a"]));
        assert_eq!(one, two);
    }

    #[test]
    fn the_process_comes_from_the_start_of_the_rule_id() {
        assert_eq!(process_of("tidy/saved#2"), "tidy");
        assert_eq!(process_of("tidy/a/b"), "tidy");
        assert_eq!(process_of("tidy"), "tidy");
    }
}
```

6. Run `cd src-tauri && cargo fmt`, then `cd src-tauri && cargo check -p terminal-grid` and `cd src-tauri && cargo check -p terminal-grid --no-default-features`. Both must succeed. Cargo updates `src-tauri/Cargo.lock`; that is expected.

## Acceptance

- [ ] `cd src-tauri && cargo test -p terminal-grid triggers::rules` shows 7 tests, all ok
- [ ] `cd src-tauri && cargo check -p terminal-grid --no-default-features` exits 0 (the app builds with every source feature off)
- [ ] `cd src-tauri && cargo test --workspace` passes
- [ ] `cd src-tauri && cargo clippy -p trigger-core -p trigger-hub -p trigger-fs --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt --check` exits 0
- [ ] `cd src-tauri && cargo clippy -p terminal-grid --all-targets 2>&1 | grep -c warning` prints the same number as the baseline in the Log, or a lower one (the app crate has existing warnings; this card must not add any)
- [ ] `git diff src-tauri/Cargo.toml` shows only the added lines from steps 2 and 3
- [ ] `git status --porcelain` lists only paths under `files:`

## Out of scope

- Calling `plan` from anywhere. T35 does.
- Adding `trigger-fs` code or a second source. Later source epics add a crate, a feature and one line in `sources()`.
- Switching the app's own dependencies to `workspace = true`.
- Changing the workspace blocks T01 added.

## If stuck

- If cargo cannot find `crates/trigger-hub` or `crates/trigger-fs`, slice B or C is not built yet. Set status blocked and say which folder is missing.
- If `trigger_core::Rule` has different fields from `id`, `kind`, `when`, read `src-tauri/crates/trigger-core/src/lib.rs` and set status blocked with what you found; do not change the crate.
- If cargo wants network access and has none, set status blocked.
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed (clippy baseline 7 warnings before the change, 7 after; triggers::rules 7 ok; workspace tests ok; fmt, no-default-features check and trigger-crate clippy ok)
- 2026-10-12 checker: pass
