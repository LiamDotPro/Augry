---
id: T24
epic: 002-trigger-internal
title: Publish tg.worktree-created and tg.worktree-removed
status: done
depends_on: [T20]
size: S
model: haiku
files:
  - src-tauri/src/git/worktree.rs
---

## Goal

`worktree_create` and `worktree_remove` publish `WorktreeCreated` and `WorktreeRemoved` after git has succeeded. `repo` is the pane's repository root, the same value `${repo.root}` expands to. Two pure helpers build the signals and are tested.

## Read first

- `plans/002-trigger-internal/epic.md` Decision 12 and `plans/002-trigger-internal/research/R01-feeds-and-shell-integration.md` implication 7
- `src-tauri/src/git/worktree.rs:1-15` the imports you extend
- `src-tauri/src/git/worktree.rs:276-300` the two commands, the feed points
- `src-tauri/src/triggers/feeds.rs` `publish` and `repo_root_of` (T20)

## Steps

1. Before changing anything, run `cd src-tauri && cargo clippy -p terminal-grid --all-targets 2>&1 | grep -c warning` and write the number in the Log. It is the baseline for the clippy check in Acceptance (it was 7 when this card was written).

2. All edits are in `src-tauri/src/git/worktree.rs`. Make them in this order.

3. Find the line `use crate::pty::TerminalId;` (there is exactly one) and replace it with these three lines:

```rust
use crate::pty::TerminalId;
use crate::triggers::feeds;
use crate::triggers::internal::InternalSignal;
```

4. Find the two lines

```rust
#[tauri::command]
pub async fn worktree_create(
```

and put these two functions directly above them. `created_signal` trims the branch the way `create` does and leaves an empty one out; both put the path in the forward slash form git and the rules use.

```rust
/// `branch` is what the user typed; `create` trims it the same way. An empty
/// name cannot get this far, but it would be left out rather than sent.
fn created_signal(repo: String, path: &str, branch: &str) -> InternalSignal {
    let branch = branch.trim();
    InternalSignal::WorktreeCreated {
        repo,
        path: normalise(path),
        branch: (!branch.is_empty()).then(|| branch.to_string()),
    }
}

fn removed_signal(repo: String, path: &str) -> InternalSignal {
    InternalSignal::WorktreeRemoved {
        repo,
        path: normalise(path),
    }
}

```

5. In `worktree_create`, find this closure body:

```rust
    crate::blocking(move || {
        let path = create(&cwd_of(&app, &id)?, &branch)?;
        refresh(&app, &id);
        Ok(path)
    })
```

and replace it with:

```rust
    crate::blocking(move || {
        let cwd = cwd_of(&app, &id)?;
        let path = create(&cwd, &branch)?;
        refresh(&app, &id);
        let state = app.state::<crate::AppState>();
        if let Some(repo) = feeds::repo_root_of(&state, &cwd) {
            feeds::publish(&app, created_signal(repo, &path, &branch));
        }
        Ok(path)
    })
```

6. In `worktree_remove`, find this closure body:

```rust
    crate::blocking(move || {
        remove(&cwd_of(&app, &id)?, &path)?;
        refresh(&app, &id);
        Ok(())
    })
```

and replace it with:

```rust
    crate::blocking(move || {
        let cwd = cwd_of(&app, &id)?;
        remove(&cwd, &path)?;
        refresh(&app, &id);
        let state = app.state::<crate::AppState>();
        if let Some(repo) = feeds::repo_root_of(&state, &cwd) {
            feeds::publish(&app, removed_signal(repo, &path));
        }
        Ok(())
    })
```

7. In the `tests` module, find the two lines

```rust
    #[test]
    fn reads_every_block() {
```

and put these three tests directly above them:

```rust
    #[test]
    fn a_created_worktree_names_the_trimmed_branch_and_a_forward_slash_path() {
        let signal = created_signal("C:/dev/app".into(), "C:\\dev\\app-x", " x ");
        assert_eq!(
            signal,
            InternalSignal::WorktreeCreated {
                repo: "C:/dev/app".into(),
                path: "C:/dev/app-x".into(),
                branch: Some("x".into()),
            }
        );
    }

    #[test]
    fn a_blank_branch_is_left_out_of_the_signal() {
        let InternalSignal::WorktreeCreated { branch, .. } =
            created_signal("/a".into(), "/b", "  ")
        else {
            panic!("expected WorktreeCreated");
        };
        assert_eq!(branch, None);
    }

    #[test]
    fn a_removed_worktree_names_its_forward_slash_path() {
        assert_eq!(
            removed_signal("/a".into(), "C:\\dev\\app-x"),
            InternalSignal::WorktreeRemoved {
                repo: "/a".into(),
                path: "C:/dev/app-x".into(),
            }
        );
    }

```

8. Run `cd src-tauri && cargo fmt`, then the commands in Acceptance.

## Acceptance

- [ ] `cd src-tauri && cargo test -p terminal-grid git::worktree` passes and the list includes the three new tests `a_created_worktree_names_the_trimmed_branch_and_a_forward_slash_path`, `a_blank_branch_is_left_out_of_the_signal` and `a_removed_worktree_names_its_forward_slash_path`
- [ ] `cd src-tauri && cargo test --workspace` passes
- [ ] `cd src-tauri && cargo fmt --check` exits 0
- [ ] `cd src-tauri && cargo check -p terminal-grid --no-default-features` exits 0
- [ ] `cd src-tauri && cargo clippy -p terminal-grid --all-targets 2>&1 | grep -c warning` prints the same number as the baseline in the Log, or a lower one (the app crate has existing warnings; this card must not add any)
- [ ] `git status --porcelain` lists only paths under `files:`, ignoring other card files under `plans/002-trigger-internal/tasks/`
- [ ] `grep -c "feeds::publish" src-tauri/src/git/worktree.rs` prints `2`

## Out of scope

- Changing `create`, `remove`, `main_root` or `sibling_path`. The signal is published from the command wrappers, which have the app handle; the plain functions stay untouched.
- Publishing for worktrees made or removed outside the app with the git command line.
- Using the main checkout as `repo`. It is the pane's own repository root, so a rule with `${repo.root}` matches.
- Fixing existing clippy warnings, renaming anything, or changing a file not listed under `files:`.

## If stuck

- If a quoted closure body is not found, set `status: blocked` and quote the two commands.
- If `cargo test` fails with a path assertion on a platform, quote it and set `status: blocked`; do not change `normalise`.
- If `cargo fmt` changes the code given above, keep the formatted result; that is expected.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed (clippy baseline 7, after 7)
- 2026-10-12 checker: pass
