# trigger-github

Fires on GitHub issues, reviews, CI runs, pull requests and releases for the signed in user.

Source id `github`. One crate, one source: everything it fires starts with
`github.`.

## Kinds

Generated from `KINDS` in `src/lib.rs`. Refresh with
`cargo run -p trigger-github --example readme` and paste the output between
the markers; `tests/readme.rs` fails while this table is stale.

<!-- kinds -->
| Kind | Title | Platforms | Permission | When | Payload |
| --- | --- | --- | --- | --- | --- |
| github.issue-assigned | Issue assigned | macOS, Windows, Linux | none | repo*, interval_s | number, title, url, author, assignees |
| github.review-requested | Review requested | macOS, Windows, Linux | none | repo*, interval_s | number, title, url, author, head, base |
| github.ci-failed | CI failed | macOS, Windows, Linux | none | repo*, branch, interval_s | id, name, branch, sha, conclusion, url, event, actor, updated_at |
| github.ci-passed | CI passed | macOS, Windows, Linux | none | repo*, branch, interval_s | id, name, branch, sha, conclusion, url, event, actor, updated_at |
| github.pr-commented | Pull request commented | macOS, Windows, Linux | none | repo*, interval_s | id, url, author, body, created_at |
| github.pr-merged | Pull request merged | macOS, Windows, Linux | none | repo*, branch, interval_s | number, title, url, author, merged_at, base |
| github.release-published | Release published | macOS, Windows, Linux | none | repo*, interval_s | tag, name, prerelease, url, published_at, author |
| github.dependency-pr-opened | Dependency update opened | macOS, Windows, Linux | none | repo*, author, interval_s | number, title, url, author, base |
<!-- /kinds -->

## Rule fields

The `when` object of a trigger, parsed by `src/rule.rs`. Unknown fields are an
error, so a typo shows up in the editor. A field a kind does not use (for
example `branch` on `github.issue-assigned`) is accepted and ignored.

| Field | Required | Default | Meaning |
|---|---|---|---|
| `repo` | yes | | The repository as `owner/name`, such as `cli/cli`. `${repo.remote}` stands for the repository of the pane's `origin` remote and is filled in by the app |
| `branch` | no | every branch | Glob for the branch. On `github.ci-failed` and `github.ci-passed` it matches the branch the run was on, on `github.pr-merged` the branch the pull request was merged into. Case sensitive, and `*` also matches `/`, so `release/*` matches `release/1/x` |
| `interval_s` | no | `120` | Seconds between checks of GitHub. Never below `60`: a smaller number is an error |
| `author` | no | `["dependabot[bot]", "renovate[bot]"]` | A list of logins. On `github.dependency-pr-opened` only pull requests opened by one of them count |

## Payload

What an event carries, available to steps as `${trigger.<field>}`. Every value
is text or a number. A comment body is cut to 200 characters.

| Kind | Fields |
|---|---|
| `github.issue-assigned` | `number`, `title`, `url`, `author` (who opened it), `assignees` (comma separated logins) |
| `github.review-requested` | `number`, `title`, `url`, `author`, `head` (source branch), `base` (target branch) |
| `github.ci-failed`, `github.ci-passed` | `id`, `name` (workflow), `branch`, `sha`, `conclusion`, `url`, `event`, `actor`, `updated_at` |
| `github.pr-commented` | `id`, `url`, `author` (who commented), `body`, `created_at` |
| `github.pr-merged` | `number`, `title`, `url`, `author`, `merged_at`, `base` |
| `github.release-published` | `tag`, `name`, `prerelease`, `url`, `published_at`, `author` |
| `github.dependency-pr-opened` | `number`, `title`, `url`, `author` (the bot), `base` |

## Platforms

| Platform | How it is detected | Limits |
|---|---|---|
| macOS | `gh` on the PATH, else in `/opt/homebrew/bin`, `/usr/local/bin`, `/usr/bin` or `~/.local/bin`. A desktop app starts with a short PATH, which is why Homebrew's folder is listed | No login shell is asked for its PATH. The login itself (keychain) does not need the shell's PATH |
| Windows | `gh.exe` on the PATH, else in `C:\Program Files\GitHub CLI` | `gh` runs without a console window |
| Linux | The same folders as macOS | |

`gh` is looked up once per run of the app and remembered. Installing it
while the app runs is noticed after a restart. Availability only says whether
`gh` was found; whether it is signed in is checked when the first rule starts.

## Permissions

None. The crate asks the OS for nothing. It uses the login that `gh auth login`
already stored, and never reads, asks for or keeps a token. To check the login
it runs `gh auth status --json hosts` (never `gh auth token`, never plain
`gh auth status`, and what they print is never logged). Every request is
`gh api`.

When `gh` is missing, or has no login, every rule shows the reason in Settings:
"GitHub CLI not found. Install it from https://cli.github.com" or "Run gh auth
login in a terminal".

## Example

```json
{ "id": "main-broke", "kind": "github.ci-failed", "when": { "repo": "cli/cli", "branch": "trunk" } }
```

## Develop

| File | What it holds |
|---|---|
| `src/lib.rs` | `KINDS`, `GithubSource` and the thread that joins the OS layer to the logic. Glue only |
| `src/rule.rs` | The typed `When` and `parse`. Pure, unit tested |
| `src/logic.rs` | Pure: which rules a `Signal` fires, with what payload, and any debounce state. Unit tested |
| `src/os.rs` | The only file that talks to the OS: `availability()` and `listen()`, per platform behind `cfg` |
| `tests/live.rs` | `#[ignore]` tests against the real OS |
| `tests/readme.rs` | Fails when the Kinds table above is stale |

```
cd src-tauri
cargo test -p trigger-github                       # pure and readme tests, any OS, offline
cargo test -p trigger-github -- --ignored          # live tests on this machine
cargo clippy -p trigger-github --all-targets -- -D warnings
```

## Known limits

What does not work, on which platform, and why.
