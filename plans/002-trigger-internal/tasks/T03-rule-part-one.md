---
id: T03
epic: 002-trigger-internal
title: Parse the when object: repo, agent and note glob
status: done
depends_on: [T02]
size: M
model: haiku
files:
  - src-tauri/src/triggers/internal/rule.rs
  - src-tauri/src/triggers/internal/mod.rs
---

## Goal

`rule::parse` turns a `tg.*` rule into a typed `Parsed` for the kinds that need no number or pattern: it checks the kind, the optional `repo` path, the `agent` and `glob` fields, and rejects every unknown field. T04 adds the exit code, duration and pattern kinds.

## Read first

- `plans/002-trigger-internal/epic.md` Decision 11
- `src-tauri/crates/trigger-fs/src/rule.rs:60-110` the same job for `fs.*`: glob building and `RuleError::Field`
- `src-tauri/crates/trigger-core/src/lib.rs:128-150` `RuleError`, `Rule`
- `src-tauri/src/triggers/internal/kinds.rs` the `when` fields of each kind (T02)

## Steps

1. Before changing anything, run `cd src-tauri && cargo clippy -p terminal-grid --all-targets 2>&1 | grep -c warning` and write the number in the Log. It is the baseline for the clippy check in Acceptance (it was 7 when this card was written).

2. Create `src-tauri/src/triggers/internal/rule.rs` with exactly this content. How it works: `Fields` holds the `when` map; each `text` call removes one field, and `finish` rejects whatever is left as an unknown field. `repo` is read first, and only for the kinds in `REPO_KINDS`, so the bell, `tg.note-saved` and `tg.app-started` reject it as unknown. Kinds that T04 handles fall through to `Filter::None` for now, which means their own fields are rejected until T04; that is expected.

```rust
//! The `when` object of a `tg.*` rule, typed per kind. Pure. Unknown fields
//! are an error, so a typo shows up in the editor.

use std::collections::BTreeMap;
use std::path::PathBuf;

use globset::{GlobBuilder, GlobMatcher};
use trigger_core::{Rule, RuleError, Value};

use super::KINDS;

/// The kinds that take a `repo` field: every pane, worktree and review kind
/// except the bell.
const REPO_KINDS: &[&str] = &[
    "tg.agent-finished",
    "tg.command-failed",
    "tg.command-slow",
    "tg.output-matched",
    "tg.task-reported",
    "tg.pane-opened-in-repo",
    "tg.worktree-created",
    "tg.worktree-removed",
    "tg.review-staged",
    "tg.focus-entered",
];

/// What a signal must satisfy besides its kind and `repo`.
#[derive(Debug, Clone)]
pub enum Filter {
    None,
    /// `tg.agent-finished`: the agent name matches, or any agent when None.
    Agent(Option<GlobMatcher>),
    /// `tg.note-saved`: the path below the notes folder matches, or any note
    /// when None.
    Note(Option<GlobMatcher>),
}

#[derive(Debug, Clone)]
pub struct Parsed {
    /// Absolute path; None means every repository.
    pub repo: Option<PathBuf>,
    pub filter: Filter,
}

fn bad(field: &str, problem: impl Into<String>) -> RuleError {
    RuleError::Field {
        field: field.into(),
        problem: problem.into(),
    }
}

/// The `when` fields not yet read. Each `take_*` removes its field, and
/// `finish` rejects whatever is left.
struct Fields(BTreeMap<String, Value>);

impl Fields {
    fn text(&mut self, name: &str) -> Result<Option<String>, RuleError> {
        match self.0.remove(name) {
            None => Ok(None),
            Some(Value::String(text)) => Ok(Some(text)),
            Some(_) => Err(bad(name, "must be a string")),
        }
    }

    fn finish(self) -> Result<(), RuleError> {
        match self.0.into_keys().next() {
            None => Ok(()),
            Some(name) => Err(bad(&name, "unknown field")),
        }
    }
}

/// Case insensitive. `literal_separator` is on for note paths, so `*` stays
/// inside one folder there, and off for agent names.
fn matcher(field: &str, pattern: &str, literal_separator: bool) -> Result<GlobMatcher, RuleError> {
    GlobBuilder::new(pattern)
        .case_insensitive(true)
        .literal_separator(literal_separator)
        .build()
        .map(|glob| glob.compile_matcher())
        .map_err(|e| bad(field, e.to_string()))
}

/// Checks that the kind belongs to this source and the fields parse. Used by
/// `check` (the editor) and by `start`, which compiles the pattern once.
pub fn parse(rule: &Rule) -> Result<Parsed, RuleError> {
    if !KINDS.iter().any(|k| k.id == rule.kind) {
        return Err(RuleError::UnknownKind(rule.kind.clone()));
    }
    let mut fields = Fields(rule.when.clone());
    let repo = if REPO_KINDS.contains(&rule.kind.as_str()) {
        match fields.text("repo")? {
            None => None,
            Some(text) => {
                let path = PathBuf::from(text);
                if !path.is_absolute() {
                    return Err(bad("repo", "must be an absolute path"));
                }
                Some(path)
            }
        }
    } else {
        None
    };
    let filter = match rule.kind.as_str() {
        "tg.agent-finished" => {
            let agent = fields.text("agent")?;
            Filter::Agent(agent.map(|a| matcher("agent", &a, false)).transpose()?)
        }
        "tg.note-saved" => {
            let glob = fields.text("glob")?;
            Filter::Note(glob.map(|g| matcher("glob", &g, true)).transpose()?)
        }
        _ => Filter::None,
    };
    fields.finish()?;
    Ok(Parsed { repo, filter })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use trigger_core::testkit::rule as make;

    fn parsed(kind: &str, when: serde_json::Value) -> Result<Parsed, RuleError> {
        parse(&make("p/t", kind, when))
    }

    fn dir() -> String {
        std::env::temp_dir().to_string_lossy().into_owned()
    }

    fn field_of(err: RuleError) -> String {
        match err {
            RuleError::Field { field, .. } => field,
            other => panic!("expected a field error, got {other:?}"),
        }
    }

    #[test]
    fn kinds_without_fields_parse_from_an_empty_when() {
        for kind in ["tg.bell", "tg.app-started", "tg.task-reported"] {
            let p = parsed(kind, json!({})).unwrap();
            assert!(p.repo.is_none(), "{kind}");
        }
    }

    #[test]
    fn an_unknown_kind_is_refused() {
        assert!(matches!(
            parsed("tg.nope", json!({})),
            Err(RuleError::UnknownKind(_))
        ));
    }

    #[test]
    fn an_unknown_field_names_the_field() {
        let err = parsed("tg.bell", json!({ "colour": "red" })).unwrap_err();
        assert_eq!(field_of(err), "colour");
    }

    #[test]
    fn the_bell_and_app_started_take_no_repo() {
        let when = json!({ "repo": dir() });
        assert_eq!(
            field_of(parsed("tg.bell", when.clone()).unwrap_err()),
            "repo"
        );
        assert_eq!(
            field_of(parsed("tg.app-started", when).unwrap_err()),
            "repo"
        );
    }

    #[test]
    fn repo_must_be_an_absolute_path() {
        let ok = parsed("tg.focus-entered", json!({ "repo": dir() })).unwrap();
        assert_eq!(ok.repo, Some(PathBuf::from(dir())));
        let relative = parsed("tg.focus-entered", json!({ "repo": "work/app" }));
        assert_eq!(field_of(relative.unwrap_err()), "repo");
        let number = parsed("tg.focus-entered", json!({ "repo": 3 }));
        assert_eq!(field_of(number.unwrap_err()), "repo");
    }

    #[test]
    fn the_pane_worktree_and_review_kinds_take_a_repo() {
        let kinds = [
            "tg.agent-finished",
            "tg.task-reported",
            "tg.pane-opened-in-repo",
            "tg.worktree-created",
            "tg.worktree-removed",
            "tg.review-staged",
            "tg.focus-entered",
        ];
        for kind in kinds {
            assert!(parsed(kind, json!({ "repo": dir() })).is_ok(), "{kind}");
        }
    }

    #[test]
    fn agent_is_a_glob_that_ignores_case() {
        let p = parsed("tg.agent-finished", json!({ "agent": "Cl*" })).unwrap();
        let Filter::Agent(Some(glob)) = p.filter else {
            panic!("expected an agent glob");
        };
        assert!(glob.is_match("claude"));
        assert!(!glob.is_match("codex"));
        let any = parsed("tg.agent-finished", json!({})).unwrap();
        assert!(matches!(any.filter, Filter::Agent(None)));
        let bad = parsed("tg.agent-finished", json!({ "agent": "[" }));
        assert_eq!(field_of(bad.unwrap_err()), "agent");
    }

    #[test]
    fn note_glob_keeps_a_star_inside_one_folder() {
        let p = parsed("tg.note-saved", json!({ "glob": "*.md" })).unwrap();
        let Filter::Note(Some(glob)) = p.filter else {
            panic!("expected a note glob");
        };
        assert!(glob.is_match("ideas.md"));
        assert!(!glob.is_match("a/ideas.md"));
        let any = parsed("tg.note-saved", json!({})).unwrap();
        assert!(matches!(any.filter, Filter::Note(None)));
    }
}
```

3. In `src-tauri/src/triggers/internal/mod.rs` find these lines:

```rust
mod kinds;

pub use kinds::KINDS;
```

and replace them with:

```rust
mod kinds;
pub mod rule;

pub use kinds::KINDS;
```

4. Run `cd src-tauri && cargo fmt`, then the commands in Acceptance.

## Acceptance

- [ ] `cd src-tauri && cargo test -p terminal-grid triggers::internal` passes and reports 11 passed
- [ ] `cd src-tauri && cargo test --workspace` passes
- [ ] `cd src-tauri && cargo fmt --check` exits 0
- [ ] `cd src-tauri && cargo check -p terminal-grid --no-default-features` exits 0
- [ ] `cd src-tauri && cargo clippy -p terminal-grid --all-targets 2>&1 | grep -c warning` prints the same number as the baseline in the Log, or a lower one (the app crate has existing warnings; this card must not add any)
- [ ] `git status --porcelain` lists only paths under `files:`

## Out of scope

- `exit_code`, `longer_than_ms` and `pattern`. T04 does them.
- Any matching of signals. `logic.rs` does that (T05).
- Checking that `repo` exists on disk. It is only checked for being absolute.
- Fixing existing clippy warnings, renaming anything, or changing a file not listed under `files:`.

## If stuck

- If a test fails with `unknown field` for `repo` on a kind that should take it, the kind is missing from `REPO_KINDS`; compare it with the list in the code above.
- If `is_none_or` or another method is reported as unstable or missing in this card or the next, run `rustc --version`; the toolchain must be 1.82 or newer. Set `status: blocked` and quote the version.
- If `cargo fmt` changes the code given above, keep the formatted result; that is expected.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed (clippy baseline 7, after 7; cargo test triggers::internal 11 passed)
- 2026-10-12 checker: pass
