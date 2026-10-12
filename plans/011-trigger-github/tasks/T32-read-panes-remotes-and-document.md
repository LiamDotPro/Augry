---
id: T32
epic: 011-trigger-github
title: Feed pane remotes to the context and document the variable
status: todo
depends_on: [T31]
size: S
model: haiku
files:
  - src-tauri/src/triggers/mod.rs
  - docs/process-triggers.md
---

## Goal

`refresh_now` collects, from the git info of every open pane, both the repository roots and the GitHub `owner/name` of the origin remote, and hands both to `Context::detect`, so a rule with `${repo.remote}` really expands in the app. `docs/process-triggers.md` lists the new variable.

## Read first

- `plans/011-trigger-github/epic.md` Decision 11
- `src-tauri/src/triggers/mod.rs:152-173` `repo_roots` and `refresh_now`
- `src-tauri/src/triggers/mod.rs:285-300` the start of the tests that use `service()`
- `src-tauri/src/triggers/variables.rs` `github_slug` (T30) and `Context::detect` (T31)
- `docs/process-triggers.md:701-703` the bullet about `variables.rs`

## Steps

1. Baseline. Run `cd src-tauri && cargo clippy -p terminal-grid --all-targets 2>&1 | grep -c warning` and write the number in the Log. It must not grow.

2. In `src-tauri/src/triggers/mod.rs` find this text exactly (it appears once):

```rust
/// Repository roots of the open panes, from the git cache where it is fresh.
fn repo_roots(state: &crate::AppState) -> Vec<String> {
    let open_dirs = state.pty.open_dirs();
    open_dirs
        .iter()
        .filter_map(|(_, cwd)| state.git.info(cwd, GIT_MAX_AGE, &open_dirs).repo_root)
        .collect()
}
```

and replace it with:

```rust
/// Repository roots and GitHub `owner/name` slugs of the open panes, from the
/// git cache where it is fresh.
fn repo_places(state: &crate::AppState) -> (Vec<String>, Vec<String>) {
    let open_dirs = state.pty.open_dirs();
    let places: Vec<(Option<String>, Option<String>)> = open_dirs
        .iter()
        .map(|(_, cwd)| {
            let info = state.git.info(cwd, GIT_MAX_AGE, &open_dirs);
            (info.repo_root, info.remote_url)
        })
        .collect();
    places_of(&places)
}

/// Splits (repository root, origin remote URL) pairs into the roots and the
/// GitHub slugs. Remotes on other hosts add nothing.
fn places_of(places: &[(Option<String>, Option<String>)]) -> (Vec<String>, Vec<String>) {
    let roots = places.iter().filter_map(|(root, _)| root.clone()).collect();
    let remotes = places
        .iter()
        .filter_map(|(_, url)| url.as_deref().and_then(variables::github_slug))
        .collect();
    (roots, remotes)
}
```

3. In the same file find this text exactly:

```rust
    let roots = if config.enabled {
        repo_roots(state)
    } else {
        Vec::new()
    };
    let ctx = Context::detect(state.notes.root(), roots, Vec::new());
```

and replace it with:

```rust
    let (roots, remotes) = if config.enabled {
        repo_places(state)
    } else {
        (Vec::new(), Vec::new())
    };
    let ctx = Context::detect(state.notes.root(), roots, remotes);
```

4. In the same file, inside `mod tests`, directly before the line `    fn refresh_applies_only_when_the_rules_change() {` and its `#[test]` line above it, add:

```rust
    #[test]
    fn places_split_roots_and_github_slugs() {
        let places = vec![
            (
                Some("/r/a".to_string()),
                Some("git@github.com:o/a.git".to_string()),
            ),
            (
                Some("/r/b".to_string()),
                Some("https://gitlab.com/o/b.git".to_string()),
            ),
            (Some("/r/c".to_string()), None),
            (None, None),
        ];
        let (roots, remotes) = places_of(&places);
        assert_eq!(roots, ["/r/a", "/r/b", "/r/c"]);
        assert_eq!(remotes, ["o/a"]);
    }

```

5. In `docs/process-triggers.md` find this text exactly:

```
- `variables.rs` fills in `${repo.root}` (every repo with a pane open, one rule
  each), `${home}`, `${downloads}`, `${notes.root}` and `${screenshots}`. A rule
  whose variable has no value here is skipped and the UI says why.
```

and replace it with:

```
- `variables.rs` fills in `${repo.root}` (every repo with a pane open, one rule
  each), `${repo.remote}` (the `owner/name` of the GitHub origin remote of every
  open pane, such as `cli/cli`, one rule each; panes with another host add
  nothing), `${home}`, `${downloads}`, `${notes.root}` and `${screenshots}`. A
  rule that uses both `${repo.root}` and `${repo.remote}` is skipped. A rule
  whose variable has no value here is skipped and the UI says why.
```

6. Run `cd src-tauri && cargo fmt`. If it changes lines you wrote, keep its version.

## Acceptance

- [ ] `cd src-tauri && cargo test -p terminal-grid places_split_roots_and_github_slugs` passes
- [ ] `cd src-tauri && cargo test -p terminal-grid triggers` passes
- [ ] `cd src-tauri && cargo fmt --check` exits 0
- [ ] `cd src-tauri && cargo clippy -p terminal-grid --all-targets 2>&1 | grep -c warning` prints a number not larger than the baseline in the Log
- [ ] `grep -c "repo_roots(state)" src-tauri/src/triggers/mod.rs` prints `0`
- [ ] `grep -c "repo.remote" docs/process-triggers.md` prints `3` or more
- [ ] `git status --porcelain` lists only paths under `files:`, ignoring other card files under `plans/011-trigger-github/tasks/`

## Out of scope

- Registering the crate (T33) or any other docs.
- Changing `GitService`, `GitInfo` or how long git answers are cached.
- Running `gh` or touching the network to check a remote. The slug comes from the URL text only.
- Fixing the app's existing clippy warnings.

## If stuck

- If a find text is not found exactly, run `grep -n "fn repo_roots\|Context::detect" src-tauri/src/triggers/mod.rs` or `grep -n "variables.rs. fills in" docs/process-triggers.md`, make the equivalent edit on what you find, and note it in the Log.
- If the compiler says `variables` is not found in `mod.rs`, the file declares it as `pub mod variables;` near the top; use `self::variables::github_slug`.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
