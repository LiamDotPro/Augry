---
id: T32
epic: 001-trigger-core
title: Add variable expansion and the screenshots folder
status: done
depends_on: [T31]
size: M
model: haiku
files:
  - src-tauri/src/triggers/variables.rs
---

## Goal

`variables.rs` can replace `${home}`, `${downloads}`, `${notes.root}`, `${screenshots}` and `${repo.root}` in a rule's `when` values: `expand(&when, &Context)` returns one rule per repository root when `${repo.root}` is used, or a skip reason when a variable has no value on this machine. `Context::detect` builds the real context, and `screenshots_dir` runs the one program per OS.

## Read first

- `plans/001-trigger-core/epic.md` Decision 12 (the variables and the `#<n>` suffix)
- `plans/001-trigger-core/research/R05-screenshots.md` the three platform bodies
- `src-tauri/src/triggers/variables.rs` the file from T31 you extend
- `src-tauri/src/pty/mod.rs:488-496` `path_string` and `home_dir`

## Steps

1. Before changing anything, run `cd src-tauri && cargo clippy -p terminal-grid --all-targets 2>&1 | grep -c warning` and write the number in the Log. It is the baseline for the clippy check in Acceptance.

2. In `src-tauri/src/triggers/variables.rs`, replace this line near the top:

```rust
use std::path::{Path, PathBuf};
```

with these lines:

```rust
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde_json::Value;
```

3. In the same file, insert the following block between the end of the function `linux_location` and the line `#[cfg(test)]` that opens `mod tests`, with one blank line before and after the block. It holds the context, the three `screenshots_dir` bodies (exactly one is compiled on each OS) and `expand`. Rule ids get `#1`, `#2` and so on, counted from 1 in the order of the sorted distinct repository roots, only when `${repo.root}` is used. A skip reason never contains a path from the user's disk.

```rust
/// Everything a variable can stand for. Built fresh for each refresh.
pub struct Context {
    pub home: PathBuf,
    pub downloads: Option<PathBuf>,
    pub notes_root: PathBuf,
    /// Repository roots of the open panes (`GitInfo.repo_root`).
    pub repo_roots: Vec<String>,
    /// Called only when a rule uses `${screenshots}`, because it may run a
    /// program. A function pointer so tests can pass a stub.
    pub screenshots: fn() -> Option<PathBuf>,
}

impl Context {
    pub fn detect(notes_root: PathBuf, repo_roots: Vec<String>) -> Context {
        Context {
            home: crate::pty::home_dir(),
            downloads: dirs::download_dir(),
            notes_root,
            repo_roots,
            screenshots: screenshots_dir,
        }
    }
}

/// The screenshots folder, only when it exists.
#[cfg(target_os = "macos")]
fn screenshots_dir() -> Option<PathBuf> {
    let raw = std::process::Command::new("defaults")
        .args(["read", "com.apple.screencapture", "location"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok());
    let dir = macos_location(raw.as_deref(), &crate::pty::home_dir());
    dir.is_dir().then_some(dir)
}

/// The screenshots folder, only when it exists.
#[cfg(target_os = "windows")]
fn screenshots_dir() -> Option<PathBuf> {
    use std::os::windows::process::CommandExt;
    // CREATE_NO_WINDOW, so no console window flashes.
    const NO_WINDOW: u32 = 0x0800_0000;
    let stdout = std::process::Command::new("reg")
        .args([
            "query",
            r"HKCU\SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\User Shell Folders",
            "/v",
            "{B7BEDE81-DF94-4682-A7D8-57A52620B86F}",
        ])
        .creation_flags(NO_WINDOW)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).into_owned());
    let dir = windows_location(stdout.as_deref(), dirs::picture_dir(), &|name| {
        std::env::var(name).ok()
    })?;
    dir.is_dir().then_some(dir)
}

/// The screenshots folder, only when it exists.
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn screenshots_dir() -> Option<PathBuf> {
    linux_location(dirs::picture_dir())
}

/// The `when` of one rule after variables are replaced.
pub type When = BTreeMap<String, Value>;

pub enum Expanded {
    /// One entry per rule. The first part is the rule id suffix: empty, or
    /// `#1`, `#2` and so on when `${repo.root}` fans out over several repositories.
    Rules(Vec<(String, When)>),
    /// The rule cannot run on this machine right now; the text says why. It
    /// never contains a value from the user's disk.
    Skipped(String),
}

/// Replaces every variable in the string values of `when`, including strings
/// inside arrays and objects.
pub fn expand(when: &When, ctx: &Context) -> Expanded {
    let value = Value::Object(when.clone().into_iter().collect());
    let fan_out = mentions(&value, REPO_ROOT);

    let roots: Vec<Option<String>> = if fan_out {
        let distinct: BTreeSet<&String> = ctx.repo_roots.iter().collect();
        if distinct.is_empty() {
            return Expanded::Skipped("no open pane is in a git repository".into());
        }
        distinct
            .into_iter()
            .map(|root| Some(root.clone()))
            .collect()
    } else {
        vec![None]
    };

    let mut rules = Vec::new();
    for (index, root) in roots.iter().enumerate() {
        match substitute(&value, ctx, root.as_deref()) {
            Ok(Value::Object(map)) => {
                let suffix = if fan_out {
                    format!("#{}", index + 1)
                } else {
                    String::new()
                };
                rules.push((suffix, map.into_iter().collect()));
            }
            Ok(_) => return Expanded::Skipped("the rule could not be read".into()),
            Err(reason) => return Expanded::Skipped(reason),
        }
    }
    Expanded::Rules(rules)
}

const REPO_ROOT: &str = "${repo.root}";

fn mentions(value: &Value, needle: &str) -> bool {
    match value {
        Value::String(text) => text.contains(needle),
        Value::Array(items) => items.iter().any(|item| mentions(item, needle)),
        Value::Object(map) => map.values().any(|item| mentions(item, needle)),
        _ => false,
    }
}

fn substitute(value: &Value, ctx: &Context, repo: Option<&str>) -> Result<Value, String> {
    Ok(match value {
        Value::String(text) => Value::String(replace_variables(text, ctx, repo)?),
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|item| substitute(item, ctx, repo))
                .collect::<Result<_, _>>()?,
        ),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, item)| Ok((key.clone(), substitute(item, ctx, repo)?)))
                .collect::<Result<_, String>>()?,
        ),
        other => other.clone(),
    })
}

fn replace_variables(text: &str, ctx: &Context, repo: Option<&str>) -> Result<String, String> {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find("${") {
        let Some(length) = rest[start..].find('}') else {
            break;
        };
        out.push_str(&rest[..start]);
        let name = &rest[start + 2..start + length];
        let replacement = match name {
            "home" => ctx.home.to_string_lossy().into_owned(),
            "notes.root" => ctx.notes_root.to_string_lossy().into_owned(),
            "downloads" => ctx
                .downloads
                .as_ref()
                .map(|dir| dir.to_string_lossy().into_owned())
                .ok_or("this machine has no downloads folder")?,
            "screenshots" => (ctx.screenshots)()
                .map(|dir| dir.to_string_lossy().into_owned())
                .ok_or("no screenshots folder was found on this machine")?,
            "repo.root" => repo
                .map(str::to_string)
                .ok_or("no open pane is in a git repository")?,
            other => return Err(format!("unknown variable ${{{other}}}")),
        };
        out.push_str(&replacement);
        rest = &rest[start + length + 1..];
    }
    out.push_str(rest);
    Ok(out)
}
```

4. At the very end of the file, after the closing `}` of `mod tests`, add one blank line and this test module:

```rust
#[cfg(test)]
mod expand_tests {
    use super::*;
    use serde_json::json;

    fn shots() -> Option<PathBuf> {
        Some(PathBuf::from("/shots"))
    }

    fn nothing() -> Option<PathBuf> {
        None
    }

    fn ctx(roots: &[&str]) -> Context {
        Context {
            home: PathBuf::from("/home/me"),
            downloads: Some(PathBuf::from("/home/me/Downloads")),
            notes_root: PathBuf::from("/notes"),
            repo_roots: roots.iter().map(|r| r.to_string()).collect(),
            screenshots: shots,
        }
    }

    fn when(value: Value) -> When {
        value.as_object().unwrap().clone().into_iter().collect()
    }

    fn only(expanded: Expanded) -> When {
        match expanded {
            Expanded::Rules(mut rules) => {
                assert_eq!(rules.len(), 1);
                let (suffix, when) = rules.remove(0);
                assert_eq!(suffix, "");
                when
            }
            Expanded::Skipped(reason) => panic!("skipped: {reason}"),
        }
    }

    fn skipped(expanded: Expanded) -> String {
        match expanded {
            Expanded::Skipped(reason) => reason,
            Expanded::Rules(_) => panic!("expected a skip"),
        }
    }

    #[test]
    fn plain_values_pass_through() {
        let input = when(json!({ "path": "/x", "settle_ms": 500, "ignore": ["a", "b"] }));
        assert_eq!(only(expand(&input, &ctx(&[]))), input);
    }

    #[test]
    fn each_variable_is_replaced() {
        let input = when(json!({
            "a": "${home}/x",
            "b": "${downloads}",
            "c": "${notes.root}/n",
            "d": "${screenshots}",
            "e": ["${home}", "plain"]
        }));
        let out = only(expand(&input, &ctx(&[])));
        assert_eq!(out["a"], json!("/home/me/x"));
        assert_eq!(out["b"], json!("/home/me/Downloads"));
        assert_eq!(out["c"], json!("/notes/n"));
        assert_eq!(out["d"], json!("/shots"));
        assert_eq!(out["e"], json!(["/home/me", "plain"]));
    }

    #[test]
    fn repo_root_makes_one_rule_per_distinct_root() {
        let input = when(json!({ "path": "${repo.root}/docs" }));
        match expand(&input, &ctx(&["/r/b", "/r/a", "/r/b"])) {
            Expanded::Rules(rules) => {
                assert_eq!(rules.len(), 2);
                assert_eq!(rules[0].0, "#1");
                assert_eq!(rules[0].1["path"], json!("/r/a/docs"));
                assert_eq!(rules[1].0, "#2");
                assert_eq!(rules[1].1["path"], json!("/r/b/docs"));
            }
            Expanded::Skipped(reason) => panic!("skipped: {reason}"),
        }
    }

    #[test]
    fn missing_values_skip_the_rule_with_a_reason() {
        let repo = when(json!({ "path": "${repo.root}" }));
        assert_eq!(
            skipped(expand(&repo, &ctx(&[]))),
            "no open pane is in a git repository"
        );

        let mut no_downloads = ctx(&[]);
        no_downloads.downloads = None;
        let downloads = when(json!({ "path": "${downloads}" }));
        assert!(skipped(expand(&downloads, &no_downloads)).contains("downloads"));

        let mut no_shots = ctx(&[]);
        no_shots.screenshots = nothing;
        let shots = when(json!({ "path": "${screenshots}" }));
        assert!(skipped(expand(&shots, &no_shots)).contains("screenshots"));
    }

    #[test]
    fn an_unknown_variable_skips_the_rule() {
        let input = when(json!({ "path": "${nope}/x" }));
        assert_eq!(
            skipped(expand(&input, &ctx(&[]))),
            "unknown variable ${nope}"
        );
    }

    #[test]
    fn an_unclosed_brace_is_left_alone() {
        let input = when(json!({ "glob": "a${b" }));
        assert_eq!(only(expand(&input, &ctx(&[])))["glob"], json!("a${b"));
    }
}
```

5. Run `cd src-tauri && cargo fmt` and then the commands in Acceptance.

## Acceptance

- [ ] `cd src-tauri && cargo test -p terminal-grid triggers::variables` shows 13 tests, all ok (seven from T31 and six new)
- [ ] `cd src-tauri && cargo fmt --check` exits 0
- [ ] `cd src-tauri && cargo clippy -p terminal-grid --all-targets 2>&1 | grep -c warning` prints the same number as the baseline in the Log, or a lower one (the app crate has existing warnings; this card must not add any)
- [ ] `git status --porcelain` lists only `src-tauri/src/triggers/variables.rs`

## Out of scope

- Calling `Context::detect` or `expand` from anywhere. T33 and T35 do that.
- Testing `screenshots_dir` itself. It runs a program, and its string handling is already tested by T31.
- Converting backslashes to forward slashes in expanded paths.
- Variables other than the five listed. An unknown `${name}` is a skip reason on purpose.
- Fixing the Windows or Linux body of `screenshots_dir` if you are on macOS: they are not compiled here. Copy them exactly.

## If stuck

- If clippy complains about `unused` code on your OS, it is a function from step 3 that only another OS uses. Do not delete it; set status blocked and quote the warning.
- If `dirs::download_dir` is not found, the `dirs` dependency changed. Set status blocked and quote the error.
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed (clippy baseline 7, after 7; variables tests 13 ok; fmt --check exit 0)
- 2026-10-12 checker: pass
