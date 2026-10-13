---
id: R02
epic: 003-trigger-git
question: How does the app's git layer (src-tauri/src/git/cli.rs) run git (binary lookup, flags, env, CREATE_NO_WINDOW, timeouts, error mapping, porcelain v2), what should trigger-git copy, and how does the app find the remote
status: answered
---

## Answer

The app runs plain `Command::new("git")` (PATH lookup only) with env `GIT_OPTIONAL_LOCKS=0`, `GIT_TERMINAL_PROMPT=0`, `GIT_PAGER=cat`, `LC_ALL=C`, null stdin, piped and thread-drained stdout and stderr, `CREATE_NO_WINDOW` on Windows, and a 3 s try_wait poll with kill. A non-zero exit is data (`ok: false`), not an error. The app never uses `-c core.quotepath=off` and never parses `status --porcelain=v2` (it uses v1 with `-z` in `review.rs`). The remote is `git remote get-url origin` only (`GitInfo.remote_url`). The crate should copy `cli.rs` as a ~70 line self contained `git.rs`.

## Evidence

All in `/Users/liamread/WebstormProjects/Terminal-Grid-003/src-tauri/src/`.

- Spawn and env: `git/cli.rs:48-58` (`Command::new("git")`, `.current_dir(cwd)`, env `GIT_OPTIONAL_LOCKS=0`, `GIT_TERMINAL_PROMPT=0`, `GIT_PAGER=cat`, `LC_ALL=C`, stdin null, stdout/stderr piped).
- Windows: `git/cli.rs:13-14` `CREATE_NO_WINDOW = 0x0800_0000`; applied at `cli.rs:60-64` via `CommandExt::creation_flags`. Same in `docs/technical-design.md` 7.2 (lines 428-438).
- Timeout: `cli.rs:16` 3 s, `POLL` 10 ms; loop `cli.rs:83-103` uses `try_wait`, on deadline `kill()` + `wait()` and returns `AppError::GitTimeout`. `run_raw` takes a caller timeout (`cli.rs:47`) for slow calls.
- Pipe draining: `cli.rs:78-81` and `132-138` read stdout and stderr on their own threads so a full pipe cannot deadlock the poll.
- Error mapping: spawn `NotFound` becomes `GitNotInstalled` ("git was not found on PATH"), other spawn errors `Io` (`cli.rs:66-76`; variants at `error.rs:25-27`). Non-zero exit is NOT an error: `GitOutput.ok` (`cli.rs:33-43`). Helpers `output` (Some on success, trailing CR/LF trimmed, `cli.rs:118-123`) and `value` (also None when empty, `cli.rs:126-130`). Callers treat None as "not available".
- Binary lookup: only the process PATH. No absolute git path resolution anywhere (grep for `set_var`, `fix_path` found nothing). `agent/installed.rs` is for agent CLIs, not git: it extends the search with the login shell PATH (`$SHELL -ilc /usr/bin/env`, 5 s timeout, `installed.rs:51-85`), `usual_dirs` (`/opt/homebrew/bin`, `/usr/local/bin`, `~/.local/bin`, ..., `installed.rs:89-107`) and de-dupes (`installed.rs:109-120`). It does not change the app's own PATH, so from the Dock git resolves via launchd's `/usr/bin:/bin:...`; on macOS `/usr/bin/git` is the Xcode shim and normally exists.
- Flags: `core.quotepath=off` is not used anywhere (grep empty). The app avoids quoting by using `-z` (`git/review.rs:230-234`: `status --porcelain=v1 -z --untracked-files=all`, parsed by splitting on `\0`, `review.rs:86-88`). Only `-c commit.gpgsign=false` appears, in tests (`review.rs:701`, `worktree.rs:404`).
- Dirty flag: `git/mod.rs:225` `status --porcelain --untracked-files=no`. Ahead/behind: `rev-list --left-right --count @{upstream}...HEAD`, output `behind<TAB>ahead` (`mod.rs:228-238`).
- Remote: `git/mod.rs:276` `remote_url: cli::value(cwd, &["remote","get-url","origin"])`, field at `mod.rs:56`, None when there is no origin. Only `origin`; no upstream remote lookup.
- Ran in a scratch repo (git 2.50.1): `GIT_OPTIONAL_LOCKS=0 LC_ALL=C git -c core.quotepath=off status --porcelain=v2 --branch -z` printed `# branch.oid <sha>|# branch.head main|? a|` (NUL separated). `git log -1 --format=%h%x00%s` printed `d2fb2db<NUL>x`. With no upstream there are no `# branch.upstream` or `# branch.ab` lines.

## Implications for the cards

Create `crates/trigger-git/src/git.rs`, no dependency on the app, std only. Outline:

```rust
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub enum GitError { NotInstalled, Timeout(String), Io(String) }
pub struct GitOutput { pub ok: bool, pub stdout: String, pub stderr: String }

/// Err only when git cannot run or times out. Non-zero exit => Ok(ok:false).
pub fn run(cwd: &Path, args: &[&str], timeout: Duration) -> Result<GitOutput, GitError> {
    let mut c = Command::new("git");
    c.args(args).current_dir(cwd)
     .env("GIT_OPTIONAL_LOCKS", "0").env("GIT_TERMINAL_PROMPT", "0")
     .env("GIT_PAGER", "cat").env("LC_ALL", "C")
     .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(windows)] { use std::os::windows::process::CommandExt; c.creation_flags(CREATE_NO_WINDOW); }
    // spawn: ErrorKind::NotFound => NotInstalled, else Io
    // take stdout/stderr, drain each on a thread (read_to_end)
    // loop try_wait, sleep 10 ms; at deadline kill()+wait() => Timeout
    // from_utf8_lossy both streams
}
pub fn value(cwd: &Path, args: &[&str]) -> Option<String> // ok and non-empty after trim
```

Rules for the crate:
- Always set `GIT_OPTIONAL_LOCKS=0` (stops `status` from refreshing and writing the index, which would fight the user's own commands) and `GIT_TERMINAL_PROMPT=0`. Use read only commands: `rev-parse`, `status`, `log`, `for-each-ref`, `rev-list`, `remote get-url`. Run `git fetch` only when the rule has `fetch: true`, with a longer timeout (e.g. 30 s). Fetch writes `FETCH_HEAD` and refs, which the watcher will see, so expect and tolerate those events.
- For anything printing paths use `-z` (as the app does) or add `-c core.quotepath=off` (the app does not). Conflict `files` count: count records starting with `u ` in `status --porcelain=v2 -z`.
- Use `status --porcelain=v2 --branch -z`. Header records: `# branch.oid`, `# branch.head`, optional `# branch.upstream`, `# branch.ab +A -B`. Entry prefixes: `1` ordinary, `2` rename (extra NUL separated original path), `u` unmerged, `?` untracked, `!` ignored. Detached HEAD shows `# branch.head (detached)`; no commits shows `# branch.oid (initial)` (from git docs, not run). Use `--untracked-files=no` for cheap polls.
- Snapshot reads: `rev-parse HEAD`, `log -1 --format=%h%x00%s`, `for-each-ref --count=<cap> --format=%(refname)%00%(objectname) refs/heads refs/tags` (cap per the epic Risks).
- Default timeout 3 s as the app; poll sleep 10 ms.
- Never call git inside a `notify` callback; send the event to a worker thread.
- `${repo.remote}`: the app only has `git remote get-url origin` (`mod.rs:276`). Do the same: `value(repo, &["remote","get-url","origin"])`, empty when none. Upstream remote would need `rev-parse --abbrev-ref --symbolic-full-name @{upstream}`; the app does not do that.
- Binary lookup: use bare `"git"` like the app. Optional macOS fallback when spawn returns NotFound: retry `/opt/homebrew/bin/git`, then `/usr/local/bin/git`. Do not copy the `installed.rs` login shell probe (spawns an interactive shell, up to 5 s).

## Doubts

- Git on PATH for a Start menu launch on Windows was not tested (cli.rs relies on it; Git for Windows adds itself to the system PATH).
- `status --porcelain=v2` records for renames, unmerged files and detached HEAD are from git docs, not exercised beyond the untracked case above.
