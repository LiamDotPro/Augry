---
id: T29
epic: 011-trigger-github
title: Remove the temporary allows and finish the README
status: done
depends_on: [T28]
size: S
model: haiku
files:
  - src-tauri/crates/trigger-github/src/logic.rs
  - src-tauri/crates/trigger-github/src/os.rs
  - src-tauri/crates/trigger-github/src/rule.rs
  - src-tauri/crates/trigger-github/src/logic/backoff.rs
  - src-tauri/crates/trigger-github/src/logic/response.rs
  - src-tauri/crates/trigger-github/README.md
---

## Goal

The crate is finished: the three temporary `#![allow(dead_code)]` lines (in `logic.rs`, `os.rs` and `rule.rs`) are gone and clippy is still clean, the leftover "slice B / slice C" wording in comments is gone, and the README's Develop and Known limits sections are written. After this card `node plans/_tools/check-crates.mjs` prints no line about `trigger-github`.

## Read first

- `src-tauri/crates/trigger-github/README.md` the whole file (the sections already filled in, and the two left to do)
- `plans/_context/conventions.md` "Trigger crates" (the eight sections and what Known limits is for)
- `src-tauri/crates/trigger-github/src/logic/backoff.rs` `waiting_until` (the one method only tests use)
- `src-tauri/crates/trigger-github/src/logic.rs` lines 1 to 8 (the comment to tidy)

## Steps

1. In `src-tauri/crates/trigger-github/src/logic.rs`, make this edit.

Find this text, which appears once:

```rust
//! comes in as arguments, so every case is a unit test. Slice B fills it in.

// Removed in slice C, when the poll loop reads everything in this file.
#![allow(dead_code)]
```

Replace it with:

```rust
//! comes in as arguments, so every case is a unit test.
```

2. In `src-tauri/crates/trigger-github/src/os.rs`, make this edit.

Find this text, which appears once:

```rust
//! `logic/response.rs`; this file only starts children.

// Removed in the last card of slice C, when the poll loop uses everything here.
#![allow(dead_code)]
```

Replace it with:

```rust
//! `logic/response.rs`; this file only starts children.
```

3. In `src-tauri/crates/trigger-github/src/rule.rs`, make this edit.

Find this text, which appears once:

```rust
//! The `when` object of a rule, typed. Pure.

// Removed in slice C, when the poll loop reads everything in this file.
#![allow(dead_code)]
```

Replace it with:

```rust
//! The `when` object of a rule, typed. Pure.
```

4. In `src-tauri/crates/trigger-github/src/logic/backoff.rs`, make this edit. `waiting_until` is only used by tests, so without the allow it must be test only.

Find this text, which appears once:

```rust
    /// When the wait ends, if one is running.
    pub fn waiting_until
```

Replace it with:

```rust
    /// When the wait ends, if one is running.
    #[cfg(test)]
    pub fn waiting_until
```

5. In `src-tauri/crates/trigger-github/src/logic/response.rs`, make this edit.

Find this text, which appears once:

```rust
/// `gh` exits 4 when there is no login. Slice C turns this message into
/// "unavailable, run gh auth login".
```

Replace it with:

```rust
/// `gh` exits 4 when there is no login. `Backoff` turns this message into
/// `Verdict::Unauthorized`, which stops the poller.
```

6. Run `cd src-tauri && cargo clippy -p trigger-github --all-targets -- -D warnings`. It must exit 0. If it reports dead code, use If stuck.

7. In `src-tauri/crates/trigger-github/README.md`, make this edit. It fills the Develop table and adds how a poll works. Do not touch any other section.

Find this text, which appears once:

```
| `src/lib.rs` | `KINDS`, `GithubSource` and the thread that joins the OS layer to the logic. Glue only |
| `src/rule.rs` | The typed `When` and `parse`. Pure, unit tested |
| `src/logic.rs` | Pure: which rules a `Signal` fires, with what payload, and any debounce state. Unit tested |
| `src/os.rs` | The only file that talks to the OS: `availability()` and `listen()`, per platform behind `cfg` |
| `tests/live.rs` | `#[ignore]` tests against the real OS |
| `tests/readme.rs` | Fails when the Kinds table above is stale |
```

Replace it with:

```
| `src/lib.rs` | `KINDS`, `GithubSource` and `start` (parse the rules, check `gh`, start the thread). Glue only |
| `src/run.rs` | The thread `trigger-github`: wakes every second, runs `Poller::tick`, sends the events. Glue only |
| `src/rule.rs` | The typed `When` and `parse`. Pure, unit tested |
| `src/logic.rs` and `src/logic/` | Pure, unit tested. `poll.rs` plans the requests and runs a tick (`Poller`), `diff.rs` with `pulls.rs`, `runs.rs`, `simple.rs` and `comments.rs` decide which rules fire, `backoff.rs` is the rate limit rules, `response.rs` reads `gh api -i` output, `cli.rs` has the `Gh` trait, the arguments and where to look for `gh`, `payload.rs`, `json.rs` and `text.rs` are helpers. `fake_gh.rs` and `testutil.rs` are for tests only |
| `src/os.rs` | The only file that starts processes: `find_gh`, `availability`, `run` (with a timeout), `GhCli` and `auth_ok` |
| `tests/fixtures/` | Trimmed GitHub answers in the shapes research note R01 captured. Tests pick items out by `id` |
| `tests/live.rs` | `#[ignore]` tests against the real GitHub through your own `gh`, read only. They skip with a message when `gh` is missing or not signed in |
| `tests/readme.rs` | Fails when the Kinds table above is stale |
| `tests/source.rs` | The public face: source id, kinds, `check` and `start` on bad rules |

How a poll works: `start` checks that `gh` exists and is signed in, then the
thread wakes every second and calls `Poller::tick`. The first tick reads your
login (`gh api user`) and plans one request per repository and group (pull
requests, workflow runs, releases, issues assigned to you, and the two comment
lists), however many rules read it. Requests run one at a time, each with the
last `ETag`; a 304 costs nothing against the rate limit. The first answer to a
request only records what exists, so arming a rule never replays history.
```

8. In `src-tauri/crates/trigger-github/README.md`, make this edit.

Find this text, which appears once:

```
cargo test -p trigger-github -- --ignored          # live tests on this machine
```

Replace it with:

```
cargo test -p trigger-github -- --ignored --nocapture   # live tests: real GitHub, read only
```

9. In `src-tauri/crates/trigger-github/README.md`, make this edit. It fills the Known limits section. It is the last section of the file.

Find this text, which appears once:

```
What does not work, on which platform, and why.
```

Replace it with:

```
- Nothing fires for what happened before a rule was armed or while the app was
  closed. The first answer to each request only records what exists, and a
  comment written before the app started never fires.
- Only the first page of each list is read: 100 pull requests, workflow runs,
  issues or comments, 10 releases. On a very busy repository, more than 100
  pull requests or runs changing within one interval can be missed.
- An issue that is unassigned and assigned again does not fire again. A pull
  request that stops asking for your review and asks again does fire again.
- `github.pr-commented` only sees comments on pull requests among the 100 most
  recently updated, and never your own comments.
- Only `github.com`. GitHub Enterprise hosts are not supported. A repository in
  an organisation with single sign on answers 403 until you run
  `gh auth refresh`; that is written to the log once.
- `gh` is looked up once per run of the app: install it, then restart. It must
  be a version that has `gh auth status --json` (tried with 2.83.2).
- If `gh` loses its login while the app runs, polling for this source stops and
  one line is written to the log. The source starts again, and then says "Run
  gh auth login in a terminal", the next time the app applies the rules.
- GitHub allows 5000 requests an hour. A rate limit makes the whole crate wait
  (60 seconds, doubling up to 15 minutes, or what GitHub asks for), and an
  offline machine retries the same way, quietly. An interval is never below 60
  seconds, a repository that answers 404 is asked five times less often, and
  with under 100 requests left every interval is four times longer.
- Problems are written as one line on stderr with rule ids and plain words
  only, never an answer from GitHub. There is no way yet to show them in
  Settings.
- Only macOS was tried when this crate was written. The Windows and Linux
  folders `gh` is looked for in are the ones its installers use.
```

10. Run `cd src-tauri && cargo fmt -p trigger-github`. It should change nothing.

## Acceptance

- [ ] `grep -rn "allow(dead_code)" src-tauri/crates/trigger-github` prints nothing
- [ ] `grep -rni "slice [abc]" src-tauri/crates/trigger-github/src src-tauri/crates/trigger-github/README.md` prints nothing
- [ ] `cd src-tauri && cargo test -p trigger-github` passes (the lib reports 167 tests on macOS and Linux)
- [ ] `cd src-tauri && cargo clippy -p trigger-github --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-github --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0
- [ ] `node plans/_tools/check-crates.mjs | grep trigger-github` prints nothing (no template, size or README warning for this crate)
- [ ] `grep -c "^## " src-tauri/crates/trigger-github/README.md` prints `8`
- [ ] `git status --porcelain -uall | grep -v "plans/011-trigger-github/tasks/"` lists only paths under `files:`

## Out of scope

- Registering the source in the app: slice D.
- Deleting code that only tests use, or rewriting tests, to satisfy the dead code check. Only the one `#[cfg(test)]` in step 4.
- Adding `#[allow(dead_code)]` anywhere, in any form.
- Editing the Kinds, Rule fields, Payload, Platforms, Permissions or Example sections.
- Changing anything in `plans/_templates/`.

## If stuck

- If a find text is not in its file, the file is not as the earlier cards left it. Set status blocked and say which edit.
- If step 6 reports dead code (`never used`) for something other than `waiting_until`, do not add an allow and do not delete the item. Set status blocked and write the item's name and file in the Log.
- If step 6 still reports `waiting_until` as unused after step 4, check that `#[cfg(test)]` is on the line directly above `pub fn waiting_until`.
- If `check-crates` prints a size warning (a file over 400 lines), paste the line in the Log and set status blocked. Do not split files in this card.
- If `cargo fmt -p trigger-github --check` fails, run `cargo fmt -p trigger-github` once and check again.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed
- 2026-10-12 checker: pass
