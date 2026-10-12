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
error, so a typo shows up in the editor.

| Field | Required | Default | Meaning |
|---|---|---|---|
| `example` | yes | | Replace with the real fields |

## Payload

What an event carries, available to steps as `${trigger.<field>}`. Paths, names,
ids and numbers only. Never file contents or secrets, never more than 4 KiB.

| Field | Meaning |
|---|---|
| `example` | Replace with the real fields |

## Platforms

| Platform | How it is detected | Limits |
|---|---|---|
| macOS | | |
| Windows | | |
| Linux | | |

## Permissions

What the OS asks the user for, and when. "None" is a fine answer.

## Example

```json
{ "id": "example", "kind": "github.example", "when": { "example": "value" } }
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
