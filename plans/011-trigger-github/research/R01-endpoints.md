---
id: R01
epic: 011-trigger-github
question: For each kind in Decision 2, which GitHub REST endpoint gives the data with the fewest requests per repo per interval, which fields are needed, and which endpoints honour ETag / If-None-Match
status: answered
---

## Answer

Four shared per repo GETs cover all eight kinds, each with an ETag:
1. `repos/{r}/pulls?state=all&sort=updated&direction=desc&per_page=100` serves review-requested, pr-merged and dependency-pr-opened.
2. `repos/{r}/actions/runs?status=completed&per_page=100` serves ci-failed and ci-passed.
3. `repos/{r}/releases?per_page=10` serves release-published.
4. `repos/{r}/issues?state=open&assignee={me}&sort=updated&direction=desc&per_page=100` serves issue-assigned.

pr-commented needs two more (`issues/comments` and `pulls/comments`, both `sort=updated&direction=desc`, with `since`). Only fetch an endpoint if at least one rule of that group exists for the repo. Search does not return an ETag, so do not use it. Notifications is a possible later optimisation, not the base.

## Evidence

Run 2026-10-12 with `gh` logged in as LiamDotPro, read only against `cli/cli`.

ETag behaviour (all `gh api -i`):
- 200 with `Etag: W/"..."`: pulls, issues, actions/runs, releases, issues/comments, pulls/comments, pulls/{n}/reviews, commits/{ref}/check-runs, commits/{ref}/status, repos/{r}/events (also `X-Poll-Interval: 60`), notifications (strong etag, `X-Poll-Interval: 60`).
- `search/issues` returned no Etag header. Not conditional, and search has its own tighter limit.
- Conditional request that worked: `gh api -i -H 'If-None-Match: W/"093a24..."' 'repos/cli/cli/pulls?state=all&per_page=1'` printed `HTTP/2.0 304 Not Modified` with headers and no body. Pass the etag verbatim including the `W/` prefix and quotes.
- Exit code on 304 is 1 and stderr is `gh: HTTP 304`. Headers still go to stdout with `-i`. The runner must treat "exit 1 plus `HTTP/2.0 304` status line" as "unchanged", not as failure.
- Note: `gh api rate_limit` showed `remaining` unchanged at 5000 after 3 x 304 and also after 3 x 200 (it may be cached or lagging), so I could not confirm empirically that 304 is free. GitHub docs state a 304 does not count against the primary rate limit (https://docs.github.com/en/rest/using-the-rest-api/best-practices-for-using-the-rest-api#use-conditional-requests-if-appropriate, not re-fetched today).
- Pagination: a plain `gh api` returns page 1 only (no `--paginate`), which is what we want for one request per poll.

Trimmed real responses (cli/cli, captured today). Keep these as fixture shapes; real list items carry many more fields, which serde must ignore.

pulls item (`repos/{r}/pulls?state=closed&sort=updated&direction=desc`):
```json
{"id":4830652819,"number":14648,"title":"Fix the unclean background colors of gh-search-code","state":"closed","merged_at":"2026-10-12T16:14:17Z","created_at":"...","updated_at":"2026-10-12T16:14:18Z","html_url":"https://github.com/cli/cli/pull/14648","user":{"login":"A4-Tacks","type":"User"},"head":{"ref":"end-search-bg"},"base":{"ref":"trunk"},"requested_reviewers":[{"login":"sergiou87"}],"requested_teams":[],"draft":false}
```
Open dependabot PR: same shape with `"user":{"login":"dependabot[bot]","type":"Bot"}`, title `chore(deps): bump github.com/mattn/go-colorable from 0.1.15 to 0.1.16`, `created_at` `2026-10-09T14:03:29Z`. Renovate is `renovate[bot]`.

issues item (`repos/{r}/issues?...&assignee=...`). PRs appear here too, marked by a `pull_request` key:
```json
{"id":5741818198,"number":14616,"title":"Clarify issue view comment output modes","state":"closed","updated_at":"2026-10-07T10:13:20Z","html_url":"https://github.com/cli/cli/pull/14616","pull_request":{"url":"..."},"user":{"login":"Copilot"},"assignees":[{"login":"babakks"},{"login":"Copilot"}],"repository_url":"https://api.github.com/repos/cli/cli"}
```

actions/runs (wrapper `{"total_count":2500,"workflow_runs":[...]}`):
```json
{"id":38303985555,"run_number":8133,"run_attempt":1,"name":"Triage Scheduled Tasks","head_branch":"trunk","head_sha":"54a671d9a8f8c7a785c132d6f6a25375dc96acc1","event":"schedule","status":"completed","conclusion":"success","html_url":"https://github.com/cli/cli/actions/runs/38303985555","created_at":"2026-10-12T20:24:14Z","updated_at":"2026-10-12T20:24:27Z","run_started_at":"2026-10-12T20:24:14Z","actor":{"login":"BagToad"}}
```

releases item:
```json
{"id":399674740,"tag_name":"v2.102.0","name":"GitHub CLI 2.102.0","draft":false,"prerelease":false,"published_at":"2026-09-30T02:40:02Z","created_at":"2026-09-30T02:18:02Z","html_url":"https://github.com/cli/cli/releases/tag/v2.102.0","author":{"login":"github-actions[bot]"}}
```

issues/comments item (the comment does not say whether the parent is a PR):
```json
{"id":6131952628,"issue_url":"https://api.github.com/repos/cli/cli/issues/13923","html_url":"https://github.com/cli/cli/issues/13923#issuecomment-6131952628","created_at":"2026-10-12T20:20:54Z","updated_at":"2026-10-12T20:29:40Z","user":{"login":"4wrxb"},"body":"..."}
```
pulls/comments item (review comments, always on a PR):
```json
{"id":4233990557,"pull_request_url":"https://api.github.com/repos/cli/cli/pulls/14568","html_url":"https://github.com/cli/cli/pull/14568#discussion_r4233990557","created_at":"2026-10-09T19:50:38Z","updated_at":"2026-10-09T19:50:38Z","user":{"login":"BagToad"}}
```

notifications item (not captured, the account has none; shape from docs): `{"id":"1","reason":"assign|review_requested|...","unread":true,"updated_at":"...","last_read_at":null,"subject":{"title":"...","url":"https://api.github.com/repos/o/r/issues/1","type":"Issue|PullRequest"},"repository":{"full_name":"o/r"}}`. Supports `If-None-Match`, `If-Modified-Since`, `X-Poll-Interval: 60`.

## Implications for the cards

Per kind, endpoint and "new since last poll" rule. State kept per repo is a small set of seen ids plus the etag per endpoint. First poll for a repo only seeds state and fires nothing.

| Kind | Endpoint (group) | Fire when | Payload fields |
|---|---|---|---|
| issue-assigned | `issues?state=open&assignee={me}&sort=updated&direction=desc&per_page=100` (G4) | item has no `pull_request` key and `id` not in seen set; then add | number, title, html_url, user.login, assignees |
| review-requested | pulls (G1) | `state=="open"`, `requested_reviewers[].login` contains me, and `id` not in seen "requested" set. Drop id from the set when it stops matching, so a re-request fires again | number, title, html_url, user.login, head.ref, base.ref |
| dependency-pr-opened | pulls (G1) | `user.login` in `{dependabot[bot], renovate[bot]}` (or equals `when.author`) and `id` not in seen set | number, title, html_url, user.login, base.ref |
| pr-merged | pulls (G1) | `merged_at != null`, id not in seen-merged set, and `base.ref` matches `when.branch` glob | number, title, html_url, user.login, merged_at, base.ref |
| ci-failed / ci-passed | `actions/runs?status=completed&per_page=100` (G2) | key `"{id}:{run_attempt}"` unseen, `head_branch` matches glob, `conclusion=="failure"` (also `timed_out`) or `"success"` | id, name, head_branch, head_sha, conclusion, html_url, event, actor.login, updated_at |
| release-published | `releases?per_page=10` (G3) | `draft==false` and `id` unseen | tag_name, name, prerelease, html_url, published_at, author.login |
| pr-commented | `issues/comments?sort=updated&direction=desc&since={t}&per_page=100` plus `pulls/comments?` same params | comment `id` unseen, `created_at >= baseline`, `user.login != me`, and parent number is a known PR | id, html_url, user.login, body (truncate), created_at |

Rules:
- Group 1 shares one request across three kinds: use `state=all&sort=updated&direction=desc&per_page=100`. Merged and opened both show up here because merging bumps `updated_at`.
- Seen sets are keyed by `id` (stable, numeric). Do not key on `updated_at`: edits change it.
- CI: `branch=` on the API is exact match only, no glob. When `when.branch` has no glob characters add `&branch={b}`; otherwise omit it and glob client side on `head_branch`. Add `&exclude_pull_requests=true` is optional. Etag differs per query string, so store etags keyed by the full path.
- pr-commented: for an issue comment, take the number from the last segment of `issue_url` and check it against the numbers seen in the group 1 response; if absent, skip (comment on an issue or an old PR). This costs no extra request. `since` is the previous poll time, ISO 8601; also a 304 on the comment endpoints is common and free.
- `{me}` is the login from `gh api user -q .login`, called once at startup, cached. It is a normal counted request, once.
- Request counts per repo per interval: 1 (rules on only one group) up to 6 (all). With 304s these are free when nothing changed. Group by (repo, endpoint path), run once, and feed every rule from the same body. This matches Decision 4.
- Runner: pass `gh api -i -H 'If-None-Match: <etag>' '<path>'`. Parse status line, headers (`etag`, `x-poll-interval`, `x-ratelimit-remaining`), blank line, body. Never use `--paginate`. Trait method returns `NotModified | Ok{etag, body} | Err`.
- Notifications option (`gh api -i 'notifications?participating=true'`, filter `reason` in `assign`, `review_requested`): one request for every repo combined, honours `X-Poll-Interval`. Not recommended as base: it depends on the user's web notification settings and read state, it returns only a subject URL (a second request is needed for details), and it omits repos the user does not watch. Mention in README as a future optimisation only.
- Do not use `search/issues`: no ETag and a separate 30 per minute limit.

## Doubts

- Free 304s not measured here (rate_limit counter looked cached); relies on GitHub docs. R02 covers limits.
- Notifications fixture is from docs, not captured.
- 100 items per page can miss events if more than 100 PRs or runs change inside one interval on a very busy repo. Acceptable at 60s or more; could follow `Link: rel=next` later.
- Org SSO repos can return 403 until `gh auth refresh` authorises the token.
