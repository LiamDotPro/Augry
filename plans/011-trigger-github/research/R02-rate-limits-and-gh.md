---
id: R02
epic: 011-trigger-github
question: GitHub's rate limits for an authenticated user and what `gh api` prints when they are hit
status: partial
---

## Answer

Primary limit is 5000 REST requests/hour per user; a 304 on an authenticated conditional request is free (verified). Secondary limits: 900 points/min for REST (GET = 1), 100 concurrent, so run requests serially. `gh api` exits 1 for every HTTP error (401, 403, 404, 429, network) and also for 304; exit 4 only means "no login found". The rate-limit error text was not reproduced live (see Doubts); detect it from `-i` headers, not message text.
Availability: find `gh` the way `agent/installed.rs` does, then run `gh auth status --json hosts` (exit 0 always, `{"hosts":{}}` means not logged in). Never `gh auth token`.

## Evidence

Machine: gh 2.83.2 at /opt/homebrew/bin/gh, logged in via keyring. Only read-only GETs were run.

Docs read 2026-10-12:
- https://docs.github.com/en/rest/using-the-rest-api/rate-limits-for-the-rest-api : 5000/h for a user token. Secondary: 100 concurrent, 900 points/min REST (GET/HEAD = 1 point, writes 5), 90s CPU per 60s, content creation about 80/min. Limit hit = 403 or 429. Primary: `x-ratelimit-remaining: 0`, wait until `x-ratelimit-reset` (epoch seconds). Secondary: if `retry-after` wait that many seconds; else if remaining is 0 wait until reset; else wait at least 60s, then exponential backoff and stop after a few tries. `GET /rate_limit` does not count against primary but can count against secondary. Continuing while limited can get the integration banned.
- https://docs.github.com/en/rest/using-the-rest-api/best-practices-for-using-the-rest-api : a 304 does not count against the primary limit when authenticated; honour `x-poll-interval` if present; make requests serially.

Run results (exit codes are real):
- 304 free: `gh api -i repos/cli/cli/releases/latest` gave 200, remaining 4995. Same call with `-H "If-None-Match: <etag>"` twice gave `HTTP/2.0 304 Not Modified`, remaining stayed 4995 both times. A plain 200 afterwards dropped it to 4994.
- 304 via gh: stderr `gh: HTTP 304`, stdout empty, exit 1 (same with and without `-i`). With `-i` the headers (including ETag) still print on stdout.
- 404: `gh api repos/cli/does-not-exist-xyz` prints the JSON body on stdout (`{"message":"Not Found",...,"status":"404"}`) and `gh: Not Found (HTTP 404)` on stderr, exit 1. With `-i`, stdout starts `HTTP/2.0 404 Not Found` and has `X-Ratelimit-Limit/Remaining/Reset/Used/Resource` headers.
- Bad token (`GH_TOKEN=bogus`): body `{"message":"Bad credentials",...,"status":"401"}`, stderr `gh: Bad credentials (HTTP 401)`, exit 1.
- Not logged in (empty `GH_CONFIG_DIR`, no GH_TOKEN/GITHUB_TOKEN): `gh api` stderr `To get started with GitHub CLI, please run:  gh auth login` plus a GH_TOKEN hint, exit 4.
- Network down (simulated with `HTTPS_PROXY=http://127.0.0.1:1`): stderr `Get "https://api.github.com/repos/cli/cli": proxyconnect tcp: dial tcp 127.0.0.1:1: connect: connection refused`, exit 1, stdout empty. No `HTTP` marker in the message, which is how to tell it from an API error.
- gh missing: `env -i PATH=/usr/bin:/bin sh -c 'gh --version'` gives `command not found`, 127. With the full path `/opt/homebrew/bin/gh` and `env -i PATH=/usr/bin:/bin HOME=$HOME`, `gh auth status` works (exit 0), so the keyring login does not need the login PATH.
- `gh auth status` logged in: exit 0, prints host, account, "Token: gho_****" (masked), scopes. Not logged in (empty config): stderr `You are not logged into any GitHub hosts. To log in, run: gh auth login`, exit 1. With an invalid GH_TOKEN: `X Failed to log in ... The token in GH_TOKEN is invalid.`, exit 1. It makes a network call to verify the token, so it is not purely local.
- `gh auth status --json hosts` (help: always exit 0 unless fatal): logged in gives `{"hosts":{"github.com":[{"state":"success","active":true,"host":"github.com","login":"...","tokenSource":"keyring","scopes":"...","gitProtocol":"https"}]}}` with no token; not logged in gives `{"hosts":{}}`, exit 0.
- Binary lookup: src-tauri/src/agent/installed.rs. `find(names, dirs, exts)` (line 15) checks `dir/name+ext` is a file; `search_dirs()` (around line 99) uses own PATH, then `login_shell_path()` (`$SHELL -ilc /usr/bin/env`, 5s timeout, line 57 on), then `usual_dirs()` (/opt/homebrew/bin, /usr/local/bin, ~/.local/bin, ...), de-duplicated. `path_exts()` handles PATHEXT on Windows. `KNOWN` (line 10) is agents only; `gh` is not in it.

## Implications for the cards

Rate limits and backoff:
- Budget: default 120s interval with about 3 requests per repo per poll is about 90/h per repo; fine. The 60s floor with many repos is still far below 5000/h as long as ETags are sent. Share one request per endpoint per repo across rules (as the epic's Risks says).
- Run every request serially (one `gh` child at a time per crate). That keeps well under the concurrency and 900 points/min limits.
- Always call `gh api -i <endpoint> -H "If-None-Match: <etag>"` and parse stdout: status line, `Etag`, `X-Ratelimit-Remaining`, `X-Ratelimit-Reset`, `Retry-After`, `X-Poll-Interval`, then blank line, then body. Header names come back capitalised oddly (`Etag`, `X-Ratelimit-Remaining`), so match case insensitively. Store the ETag per (repo, endpoint) in memory only.
- Treat exit 1 with status line `304` as "unchanged", not an error. Use the status line from `-i` stdout (it prints even on errors), not the stderr text.
- Rate limited = status 403 or 429 AND (`Retry-After` present, or `X-Ratelimit-Remaining: 0`, or body message contains "rate limit"). A plain 403 without those is a permissions or SSO problem: report it, do not back off as a limit.
- Wait: `Retry-After` seconds if present; else until `X-Ratelimit-Reset` if remaining is 0; else 60s. Repeated secondary failures: double the wait (60, 120, 240, capped at 15 min). Reset the backoff on the next 200 or 304. Apply the wait to the whole crate, not one rule. Never poll during the wait.
- If `X-Poll-Interval` is larger than the rule's `interval_s`, use it for that endpoint.
- Proactive: if `X-Ratelimit-Remaining` is under 100, stretch every interval to 4x until reset.
- Other errors: 404 means repo missing or no access; surface once per rule and poll at 5x the interval. 401 (or exit 4) means `Unsupported` again with the login command. Transport errors (exit 1, no HTTP status line in the `-i` output) use the same exponential backoff, quietly.
- Do not use `gh api rate_limit` to pace; response headers are authoritative and that call can count toward secondary limits.

Availability check (no tokens):
1. Resolve `gh`: reuse the installed.rs approach (own PATH, then login shell PATH, then usual dirs, adding `/opt/homebrew/bin`, `/usr/local/bin`, `/usr/bin`, `~/.local/bin`; on Windows `C:\Program Files\GitHub CLI`). The crate depends only on trigger-core, so it cannot call `crate::agent::installed`. Recommend the app resolves the directory list or the `gh` path (the 5s login shell spawn already lives there) and passes it into the runner constructor. Spawn the child by absolute path, not by name.
2. Missing: no file found, so `Unsupported("GitHub CLI not found. Install it from https://cli.github.com")`.
3. Not logged in: run `<gh> auth status --json hosts` with a timeout of about 10s (it can touch the network). Exit 0 and `hosts` containing `github.com` with `state` `success` means available. Empty `hosts` or a non-success state means `Unsupported("Run `gh auth login`")`. A non-zero exit, spawn failure or timeout is not "not logged in": treat as transient and retry next tick.
4. Do not call `gh auth token` or `gh auth status --show-token`. Do not log stdout of plain `gh auth status` (it prints a masked token prefix).
5. Set `GH_PROMPT_DISABLED=1` and `GH_NO_UPDATE_NOTIFIER=1` on the child and null stdin so gh never waits for input. (Env names from gh docs memory; not exercised here.)
6. Recheck availability only on failure (401, exit 4) or when rules change, not every poll.

## Doubts

- status is partial: the exact stderr and body for a real primary or secondary rate limit were not reproduced; doing so would burn quota or trip abuse detection. Documented shape: 403/429, message like "API rate limit exceeded for user ID ..." or "You have exceeded a secondary rate limit"; gh then prints `gh: <message> (HTTP 403)` and exits 1 (inferred from the verified 401/404 pattern).
- GitHub Enterprise Server hosts may differ; only github.com was tested.
- Windows and Linux lookups were not run; inferred from installed.rs.
- `GH_PROMPT_DISABLED` and `GH_NO_UPDATE_NOTIFIER` untested.
- An early `gh api rate_limit` reading showed `used: 0` while response headers showed 246 used, so that endpoint seems cached or lagging. Trust response headers.
