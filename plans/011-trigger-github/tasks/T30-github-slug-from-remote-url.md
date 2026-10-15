---
id: T30
epic: 011-trigger-github
title: Add github_slug to variables.rs
status: done
depends_on: [T29]
size: S
model: haiku
files:
  - src-tauri/src/triggers/variables.rs
---

## Goal

`variables.rs` has a pure function `github_slug(url) -> Option<String>` that turns a git remote URL into `owner/name` for `github.com` (https, http, ssh, git and the `git@github.com:o/n.git` form) and into None for every other host or shape. Nothing calls it yet; T31 and T32 use it. A token or password inside the URL never appears in the answer.

## Read first

- `plans/011-trigger-github/epic.md` Decision 11
- `src-tauri/src/triggers/variables.rs:104-119` `linux_location`, the function the new one goes after, and the `Context` doc comment below it
- `src-tauri/src/git/mod.rs:56` `GitInfo.remote_url` is `git remote get-url origin`, a raw string

## Steps

1. Baseline. Run `cd src-tauri && cargo test -p terminal-grid variables 2>&1 | grep "test result"` and write the first result line in the Log. (The first build of the app crate takes several minutes.)

2. Make this edit in `src-tauri/src/triggers/variables.rs`.

Edit 1. Find this text exactly (it appears once):

```rust
/// Everything a variable can stand for. Built fresh for each refresh.
```

Replace it with:

```rust
/// `owner/name` of a GitHub remote URL, None for any other host or shape.
/// Reads https, http, ssh and git URLs and the scp form
/// `git@github.com:owner/name.git`. A user name, password or token in the URL
/// is dropped and never returned.
pub fn github_slug(url: &str) -> Option<String> {
    let url = url.trim();
    let (host, path) = match url.split_once("://") {
        Some((scheme, rest)) => {
            let scheme = scheme.to_ascii_lowercase();
            if !matches!(scheme.as_str(), "https" | "http" | "ssh" | "git") {
                return None;
            }
            let (authority, path) = rest.split_once('/')?;
            let host = authority.rsplit('@').next()?;
            (host.split(':').next()?, path)
        }
        None => {
            let (authority, path) = url.split_once(':')?;
            (authority.rsplit('@').next()?, path)
        }
    };
    if !host.eq_ignore_ascii_case("github.com") {
        return None;
    }
    let path = path.trim_end_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    let (owner, name) = path.split_once('/')?;
    let word = |part: &str| {
        !part.is_empty()
            && part != "."
            && part != ".."
            && part
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    };
    (word(owner) && word(name)).then(|| format!("{owner}/{name}"))
}

/// Everything a variable can stand for. Built fresh for each refresh.
```

3. Append this test module at the very end of the same file, after the last closing brace of `mod expand_tests`, with one blank line before it:

```rust
#[cfg(test)]
mod slug_tests {
    use super::*;

    #[test]
    fn github_urls_give_owner_and_name() {
        for url in [
            "https://github.com/cli/cli",
            "https://github.com/cli/cli.git",
            "https://github.com/cli/cli/",
            "http://github.com/cli/cli.git",
            "https://GitHub.com/cli/cli.git",
            "https://user:ghp_secret@github.com/cli/cli.git",
            "git@github.com:cli/cli.git",
            "git@github.com:cli/cli",
            "ssh://git@github.com/cli/cli.git",
            "ssh://git@github.com:22/cli/cli.git",
            "git://github.com/cli/cli.git",
            "  https://github.com/cli/cli.git\n",
        ] {
            assert_eq!(github_slug(url), Some("cli/cli".into()), "{url}");
        }
        assert_eq!(
            github_slug("git@github.com:my-org/my_repo.js.git"),
            Some("my-org/my_repo.js".into())
        );
    }

    #[test]
    fn other_hosts_and_shapes_give_nothing() {
        for url in [
            "",
            "https://gitlab.com/cli/cli.git",
            "git@gitlab.com:cli/cli.git",
            "https://github.com.evil.example/cli/cli.git",
            "https://evil.example/github.com/cli/cli.git",
            "https://github.com/cli",
            "https://github.com/cli/",
            "https://github.com/cli/cli/pulls",
            "https://github.com/cli/cli extra",
            "https://github.com/../cli",
            "ftp://github.com/cli/cli.git",
            "file:///srv/git/cli.git",
            "/home/me/code/cli",
            "C:\\code\\cli",
            "../cli",
        ] {
            assert_eq!(github_slug(url), None, "{url}");
        }
    }

    #[test]
    fn a_token_in_the_url_never_reaches_the_answer() {
        let slug = github_slug("https://x-access-token:ghs_abc123@github.com/o/n.git").unwrap();
        assert_eq!(slug, "o/n");
        assert!(!slug.contains("ghs_"));
    }
}
```

4. Run `cd src-tauri && cargo fmt`. It should change nothing.

## Acceptance

- [ ] `cd src-tauri && cargo test -p terminal-grid slug_tests` passes (3 tests)
- [ ] `cd src-tauri && cargo test -p terminal-grid variables` passes and shows 3 more tests than the Log baseline
- [ ] `cd src-tauri && cargo fmt --check` exits 0
- [ ] `cd src-tauri && cargo clippy -p terminal-grid --all-targets 2>&1 | grep -c "variables.rs"` prints `0`
- [ ] `git status --porcelain` lists only paths under `files:`, ignoring other card files under `plans/011-trigger-github/tasks/`

## Out of scope

- Using `github_slug` anywhere. `Context`, `expand` and `mod.rs` are T31 and T32.
- Other hosts (GitHub Enterprise, `www.github.com`, GitLab). Only `github.com`.
- Changing the existing tests or the existing clippy warnings.

## If stuck

- If the find text of step 2 is not found, `variables.rs` has changed; run `grep -n "Everything a variable can stand for" src-tauri/src/triggers/variables.rs`, put the function on the line above that doc comment, and note it in the Log.
- If a test in step 3 fails, do not change the expected values; show the failing URL in the Log and set status blocked.
- If `cargo fmt` changes lines you wrote, keep its version.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed (baseline `variables` 13 passed; after 16 passed, slug_tests 3 passed; fmt --check 0; clippy variables.rs 0)
- 2026-10-12 checker: pass
