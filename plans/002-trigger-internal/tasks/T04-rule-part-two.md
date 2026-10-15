---
id: T04
epic: 002-trigger-internal
title: Parse exit codes, durations and output patterns
status: done
depends_on: [T03]
size: M
model: haiku
files:
  - src-tauri/src/triggers/internal/rule.rs
---

## Goal

`rule::parse` handles all thirteen kinds: `exit_code` for `tg.command-failed`, `longer_than_ms` and `exit_code` for `tg.command-slow`, and `pattern` for `tg.output-matched`, compiled with `regex` 1.13, case insensitive, with a 256 KiB size limit. A bad pattern is a `RuleError`, never a panic.

## Read first

- `plans/002-trigger-internal/epic.md` Decision 11
- `plans/002-trigger-internal/research/R02-regex.md` the exact `RegexBuilder` call and the size limit test
- `src-tauri/src/triggers/internal/rule.rs` the file you replace (T03)

## Steps

1. Before changing anything, run `cd src-tauri && cargo clippy -p terminal-grid --all-targets 2>&1 | grep -c warning` and write the number in the Log. It is the baseline for the clippy check in Acceptance (it was 7 when this card was written).

2. Replace the whole of `src-tauri/src/triggers/internal/rule.rs` with exactly this content. Compared with T03 it adds the `Failed`, `Slow` and `Pattern` variants of `Filter`, the `int` and `uint` readers, the `REGEX_SIZE_LIMIT` constant, three more arms in `parse`, and tests. The pattern is compiled here in `parse`, so `InternalSource::start` (T07) compiles it once per rule.

```rust
//! The `when` object of a `tg.*` rule, typed per kind. Pure. Unknown fields
//! are an error, so a typo shows up in the editor.

use std::collections::BTreeMap;
use std::path::PathBuf;

use globset::{GlobBuilder, GlobMatcher};
use regex::{Regex, RegexBuilder};
use trigger_core::{Rule, RuleError, Value};

use super::KINDS;

/// Compiled patterns bigger than this are refused.
const REGEX_SIZE_LIMIT: usize = 256 * 1024;

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
    /// `tg.command-failed`: this exit code, or any code except 0 when None.
    Failed {
        exit_code: Option<i64>,
    },
    /// `tg.command-slow`: ran for more than `longer_than_ms`, and this exit
    /// code when set.
    Slow {
        longer_than_ms: u64,
        exit_code: Option<i64>,
    },
    /// `tg.output-matched`: the line matches.
    Pattern(Regex),
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

    fn int(&mut self, name: &str) -> Result<Option<i64>, RuleError> {
        match self.0.remove(name) {
            None => Ok(None),
            Some(value) => value
                .as_i64()
                .map(Some)
                .ok_or_else(|| bad(name, "must be a whole number")),
        }
    }

    fn uint(&mut self, name: &str) -> Result<Option<u64>, RuleError> {
        match self.0.remove(name) {
            None => Ok(None),
            Some(value) => value
                .as_u64()
                .map(Some)
                .ok_or_else(|| bad(name, "must be a whole number, 0 or more")),
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
        "tg.command-failed" => Filter::Failed {
            exit_code: fields.int("exit_code")?,
        },
        "tg.command-slow" => Filter::Slow {
            longer_than_ms: fields
                .uint("longer_than_ms")?
                .ok_or_else(|| bad("longer_than_ms", "is required"))?,
            exit_code: fields.int("exit_code")?,
        },
        "tg.output-matched" => {
            let pattern = fields
                .text("pattern")?
                .ok_or_else(|| bad("pattern", "is required"))?;
            let regex = RegexBuilder::new(&pattern)
                .case_insensitive(true)
                .size_limit(REGEX_SIZE_LIMIT)
                .build()
                .map_err(|e| bad("pattern", e.to_string()))?;
            Filter::Pattern(regex)
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
    fn every_pane_worktree_and_review_kind_takes_a_repo() {
        for kind in REPO_KINDS {
            let when = match *kind {
                "tg.command-slow" => json!({ "repo": dir(), "longer_than_ms": 1 }),
                "tg.output-matched" => json!({ "repo": dir(), "pattern": "x" }),
                _ => json!({ "repo": dir() }),
            };
            assert!(parsed(kind, when).is_ok(), "{kind}");
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

    #[test]
    fn command_failed_defaults_to_any_non_zero_code() {
        let any = parsed("tg.command-failed", json!({})).unwrap();
        assert!(matches!(any.filter, Filter::Failed { exit_code: None }));
        let two = parsed("tg.command-failed", json!({ "exit_code": 2 })).unwrap();
        assert!(matches!(two.filter, Filter::Failed { exit_code: Some(2) }));
        let text = parsed("tg.command-failed", json!({ "exit_code": "2" }));
        assert_eq!(field_of(text.unwrap_err()), "exit_code");
    }

    #[test]
    fn command_slow_needs_longer_than_ms() {
        let missing = parsed("tg.command-slow", json!({ "exit_code": 0 }));
        assert_eq!(field_of(missing.unwrap_err()), "longer_than_ms");
        let negative = parsed("tg.command-slow", json!({ "longer_than_ms": -5 }));
        assert_eq!(field_of(negative.unwrap_err()), "longer_than_ms");
        let ok = parsed(
            "tg.command-slow",
            json!({ "longer_than_ms": 30000, "exit_code": 0 }),
        )
        .unwrap();
        assert!(matches!(
            ok.filter,
            Filter::Slow {
                longer_than_ms: 30000,
                exit_code: Some(0)
            }
        ));
    }

    #[test]
    fn output_pattern_is_required_and_ignores_case() {
        let missing = parsed("tg.output-matched", json!({}));
        assert_eq!(field_of(missing.unwrap_err()), "pattern");
        let p = parsed("tg.output-matched", json!({ "pattern": "error: .*failed" })).unwrap();
        let Filter::Pattern(regex) = p.filter else {
            panic!("expected a pattern");
        };
        assert!(regex.is_match("ERROR: build FAILED"));
        assert!(!regex.is_match("all good"));
    }

    #[test]
    fn a_bad_or_oversized_pattern_is_refused_with_the_reason() {
        let syntax = parsed("tg.output-matched", json!({ "pattern": "(" }));
        assert_eq!(field_of(syntax.unwrap_err()), "pattern");
        let huge = parsed("tg.output-matched", json!({ "pattern": r"(\w{50}){50}" }));
        match huge.unwrap_err() {
            RuleError::Field { field, problem } => {
                assert_eq!(field, "pattern");
                assert!(problem.contains("size limit"), "{problem}");
            }
            other => panic!("expected a field error, got {other:?}"),
        }
    }
}
```

3. Run `cd src-tauri && cargo fmt`, then the commands in Acceptance.

## Acceptance

- [ ] `cd src-tauri && cargo test -p terminal-grid triggers::internal` passes and reports 15 passed
- [ ] `cd src-tauri && cargo test --workspace` passes
- [ ] `cd src-tauri && cargo fmt --check` exits 0
- [ ] `cd src-tauri && cargo check -p terminal-grid --no-default-features` exits 0
- [ ] `cd src-tauri && cargo clippy -p terminal-grid --all-targets 2>&1 | grep -c warning` prints the same number as the baseline in the Log, or a lower one (the app crate has existing warnings; this card must not add any)
- [ ] `git status --porcelain` lists only paths under `files:`

## Out of scope

- Matching a line against the regex. `logic.rs` does that (T05).
- Lowering the size limit, turning Unicode off, or adding regex flags.
- Truncating long patterns.
- Fixing existing clippy warnings, renaming anything, or changing a file not listed under `files:`.

## If stuck

- If the test `a_bad_or_oversized_pattern_is_refused_with_the_reason` fails because the message does not contain `size limit`, run `cd src-tauri && cargo tree -p terminal-grid -i regex` and write the `regex` version in the Log. If it is not 1.13.x, set `status: blocked`. Do not change the test text.
- If `cargo test` complains about an unresolved `regex` or `globset`, T01 was not applied; set `status: blocked`.
- If `cargo fmt` changes the code given above, keep the formatted result; that is expected.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed (clippy baseline 7, after 7; regex 1.13.1 size-limit test passed)
- 2026-10-12 checker: pass
