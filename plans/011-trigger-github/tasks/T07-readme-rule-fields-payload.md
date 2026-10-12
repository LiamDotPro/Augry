---
id: T07
epic: 011-trigger-github
title: Write the README Rule fields, Payload and Example sections
status: done
depends_on: [T06]
size: S
model: haiku
files:
  - src-tauri/crates/trigger-github/README.md
---

## Goal

`README.md` describes the `when` fields as `rule.rs` now parses them (this covers the changes of T03 to T06), the payload of each kind, and a real example. Platforms, Permissions, Develop and Known limits are slice C's.

## Read first

- `src-tauri/crates/trigger-github/README.md` the file to edit
- `src-tauri/crates/trigger-github/src/rule.rs` what each field accepts
- `src-tauri/crates/trigger-github/src/lib.rs` the payload field docs

## Steps

1. In `src-tauri/crates/trigger-github/README.md`, replace everything from the line `## Rule fields` up to, but not including, the line `## Platforms` with exactly this text (it ends with a blank line):

````markdown
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
````

2. In the same file, replace everything from the line `## Example` up to, but not including, the line `## Develop` with exactly this text (it ends with a blank line):

````markdown
## Example

```json
{ "id": "main-broke", "kind": "github.ci-failed", "when": { "repo": "cli/cli", "branch": "trunk" } }
```
````

3. Do not touch the Kinds section, Platforms, Permissions, Develop or Known limits.

## Acceptance

- [ ] `grep -c "Replace with" src-tauri/crates/trigger-github/README.md` prints `0`
- [ ] `grep -n "^## " src-tauri/crates/trigger-github/README.md` lists, in order: Kinds, Rule fields, Payload, Platforms, Permissions, Example, Develop, Known limits
- [ ] `cd src-tauri && cargo test -p trigger-github` passes (the README table test must still pass)
- [ ] `node plans/_tools/check-crates.mjs` exits 0 (warnings that `lib.rs` and `os.rs` still have template code are expected)
- [ ] `git status --porcelain` lists only paths under `files:`

## Out of scope

- Platforms, Permissions, Develop, Known limits: slice C writes them when the OS code exists.
- Changing the Kinds table (it is generated).
- Any Rust change.

## If stuck

- If `## Platforms` or `## Develop` is not in the README, the file is not as the scaffold made it. Set status blocked.
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed
- 2026-10-12 checker: pass
