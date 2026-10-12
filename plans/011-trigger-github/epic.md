---
id: 011-trigger-github
title: trigger-github
status: carding
depends_on: [001-trigger-core]
---

# 011 trigger-github

## Goal

The crate `src-tauri/crates/trigger-github` fires on things that happen on
GitHub for the signed in user: an issue assigned, a review requested, CI
failing or passing on a branch, a comment on a pull request, a merge, a release
in a watched repository, and a Dependabot or Renovate pull request opening. It
polls with the user's own `gh` login and keeps nothing but the last seen ids.

## Non goals

- Pushing anything to GitHub. Other services (Linear, Jira, Sentry, Slack),
  which follow this crate's pattern later.

## Decisions

1. **Crate** `trigger-github`, source id `github`, depends on `trigger-core`
   only. It runs `gh api` as a child process for every request, so it uses the
   user's existing login and never handles a token itself.
2. **Kinds**: `github.issue-assigned`, `github.review-requested`,
   `github.ci-failed`, `github.ci-passed`, `github.pr-commented`,
   `github.pr-merged`, `github.release-published`,
   `github.dependency-pr-opened`.
3. **`when`**: `repo` (`owner/name`, or `${repo.remote}` resolved by the app
   from the pane's origin), `branch` (glob), `interval_s` (default 120, never
   below 60), `author` for dependency bots.
4. **Polling** uses conditional requests (`If-None-Match`) where the API
   supports them, and one request per rule per interval at most.
5. **Availability** is `Unsupported` when `gh` is missing or not logged in,
   with the command to run.

6. **Finding `gh`** is the crate's own job, in `os.rs`, so the crate stays
   isolated and registers as a plain `GithubSource`: first the process PATH,
   then the usual folders (`/opt/homebrew/bin`, `/usr/local/bin`, `/usr/bin`,
   `~/.local/bin`, on Windows `C:\Program Files\GitHub CLI`). No login shell
   probe. The path is looked up once and cached in a `OnceLock`.
   `availability()` only says whether `gh` was found (cheap, no child process),
   because the catalogue command calls it on the UI thread.
7. **Login is checked in `start`**: `gh auth status --json hosts` with a 10 s
   timeout (R02). No `github.com` host with state `success` is
   `SourceError::Unavailable("Run gh auth login in a terminal")`, so Settings
   shows it on every rule. A timeout or spawn failure is `SourceError::Os`.
   Never `gh auth token`, never log `gh auth status` output.
8. **The runner** is a trait so the poll loop is tested with a fake:
   `trait Gh: Send { fn get(&self, path: &str, etag: Option<&str>) -> GhResponse; fn login(&self) -> Option<String>; }`.
   `os.rs` implements it with `gh api -i [-H "If-None-Match: <etag>"] <path>`,
   env `GH_PROMPT_DISABLED=1` and `GH_NO_UPDATE_NOTIFIER=1`, null stdin,
   `CREATE_NO_WINDOW` on Windows, a 30 s timeout, one child at a time. The
   `-i` output (status line, headers, blank line, body) is parsed by a pure
   function in `logic`, never from stderr text. Exit 1 with a `304` status
   line is `NotModified`.
9. **Requests and fire rules** follow R01's table exactly: four shared
   endpoint groups per repo (pulls, runs, releases, issues assigned to me) plus
   the two comment endpoints, each fetched only when a rule of that group
   exists for the repo, once per (repo, path) per interval, fed to every rule.
   ETags are kept per full path, in memory. Seen sets are keyed by `id`
   (`id:run_attempt` for runs). The first poll of a repo only seeds state.
   `{me}` comes from `gh api user` once per start.
10. **Backoff** follows R02's rules exactly (rate limited means 403 or 429 with
    `Retry-After` or `X-Ratelimit-Remaining: 0` or "rate limit" in the body;
    waits 60, 120, 240 s capped at 15 min, applied to the whole crate; 404
    polls at 5x; under 100 remaining stretches intervals 4x). It is a pure
    struct with a fake clock in tests.
11. **`${repo.remote}`** is a new app variable (the last slice): `owner/name`
    parsed from each open pane's `GitInfo.remote_url` for `github.com` remotes
    (https and ssh forms), fanned out like `${repo.root}` with the same `#<n>`
    suffix. A rule using both `${repo.root}` and `${repo.remote}` is skipped
    with a reason.
12. **Registration**: cargo feature `trigger-github` (default on) in
    `src-tauri/Cargo.toml` and one line in `sources()` in
    `src-tauri/src/triggers/mod.rs`, exactly like `trigger-fs`.
13. **Payload** per kind is R01's table. A comment body is cut to 200
    characters. `when.interval_s` under 60 is a `RuleError`; `when.repo` must
    look like `owner/name`; `when.branch` is a glob (globset, as in
    `trigger-fs`) matched client side; `when.author` defaults to
    `dependabot[bot]` and `renovate[bot]`.

## Shape

```
src-tauri/crates/trigger-github/
  src/lib.rs            KINDS (8), GithubSource, start (auth check, thread "trigger-github")
  src/rule.rs           When { repo, branch, interval_s, author }, parse
  src/logic.rs          mod root: pub use of the pieces below, Armed, State
  src/logic/response.rs parse_gh_output(stdout: &[u8], exit: Option<i32>) -> GhResponse
  src/logic/backoff.rs  Backoff (pure, from R02)
  src/logic/diff.rs     Group, path_for(repo, group, me, since), diff(group, body, &mut RepoState, rules, me) -> fires
  src/logic/tests.rs    fixture tests
  src/os.rs             find_gh(), GhCli: Gh, auth_ok()
  tests/fixtures/*.json trimmed bodies captured in R01
  tests/live.rs         #[ignore], skipped with a message when gh is missing or logged out
```

```rust
pub enum GhResponse {
    Ok { status: u16, etag: Option<String>, headers: Vec<(String, String)>, body: String },
    NotModified { headers: Vec<(String, String)> },
    Http { status: u16, headers: Vec<(String, String)>, body: String },
    Transport(String),
}
pub enum Group { Pulls, Runs, Releases, AssignedIssues, IssueComments, PullComments }
```

The poll thread wakes every second, polls whatever is due (serially), applies
`Backoff`, and sends events. Dropping the handle stops it within a second.

## Slices

| Slice | Covers | Depends on |
|---|---|---|
| A (T01 to T09) | scaffold from the template, KINDS, `rule.rs` with tests, README Kinds table | |
| B (T10 to T19) | `logic`: response parsing, backoff, per group diff with fixture JSON, payloads | A |
| C (T20 to T29) | `os.rs` (find `gh`, `GhCli`, auth check), the poll loop with a fake `Gh`, live tests, README | B |
| D (T30 to T39) | app: feature and `sources()` line, `${repo.remote}` in `variables.rs`, repo map row | C |

## Open questions

| Id | Question | Why it matters | Answer (filled after research) |
|---|---|---|---|
| R01 | The GitHub REST endpoints and fields for each kind, and which support ETags | Decision 2 and 4 | Four shared GETs per repo plus two comment endpoints, all with ETags; see Decisions 9. Fixtures captured from cli/cli. |
| R02 | GitHub's rate limits for an authenticated user and what `gh api` prints when they are hit | Decision 3 | 5000/h, 304 free, serial requests; `gh api` exits 1 for all HTTP errors and 304, so parse `-i` output. Partial: real rate limit text not reproduced. See Decisions 7, 8, 10. |

## Risks

- A user with many rules and short intervals hits rate limits; the crate
  shares one request per repo per interval across rules.
