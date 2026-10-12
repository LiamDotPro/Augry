---
id: 011-trigger-github
title: trigger-github
status: draft
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

## Shape

Written during planning: `KINDS`, `When`, pure "what is new since the last
poll" logic over fixture JSON, the `gh` runner behind a trait.

## Slices

| Slice | Covers | Depends on |
|---|---|---|
| A | kinds, `When`, diff logic with fixtures from the GitHub API | |
| B | the `gh` runner and the poll loop with a fake runner | A |
| C | live tests (skipped without `gh`), README, feature flag, registration, repo map | B |

## Open questions

| Id | Question | Why it matters | Answer (filled after research) |
|---|---|---|---|
| R01 | The GitHub REST endpoints and fields for each kind, and which support ETags | Decision 2 and 4 | |
| R02 | GitHub's rate limits for an authenticated user and what `gh api` prints when they are hit | Decision 3 | |

## Risks

- A user with many rules and short intervals hits rate limits; the crate
  shares one request per repo per interval across rules.
