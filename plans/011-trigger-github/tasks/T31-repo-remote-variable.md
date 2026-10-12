---
id: T31
epic: 011-trigger-github
title: Resolve the repo.remote variable in expand
status: done
depends_on: [T30]
size: M
model: haiku
files:
  - src-tauri/src/triggers/variables.rs
  - src-tauri/src/triggers/rules.rs
  - src-tauri/src/triggers/mod.rs
---

## Goal

`expand` replaces `${repo.remote}` with the `owner/name` of each distinct GitHub origin in `Context.repo_remotes`, one rule per slug with the suffixes `#1`, `#2` as for `${repo.root}`. With no GitHub remote the rule is skipped with a reason, and a rule using both `${repo.root}` and `${repo.remote}` is skipped with a reason. `mod.rs` passes an empty remote list for now; T32 fills it in.

## Read first

- `plans/011-trigger-github/epic.md` Decision 11
- `src-tauri/src/triggers/variables.rs:120-145` `Context` and `detect`
- `src-tauri/src/triggers/variables.rs:195-290` `expand`, `mentions`, `substitute`, `replace_variables`
- `src-tauri/src/triggers/rules.rs:97-106` the test `ctx` helper
- `src-tauri/src/triggers/mod.rs:162-173` `refresh_now`, and `:256-264` the test `ctx`

## Steps

1. Baseline. Run `cd src-tauri && cargo clippy -p terminal-grid --all-targets 2>&1 | grep -c warning` and write the number in the Log. It must not grow.

2. Make these edits in `src-tauri/src/triggers/variables.rs`, in this order.

Edit 1. Find this text exactly (it appears once):

```rust
`${notes.root}`, `${screenshots}` and `${repo.root}`. Design:
```

Replace it with:

```rust
`${notes.root}`, `${screenshots}`, `${repo.root}` and `${repo.remote}`. Design:
```

Edit 2. Find this text exactly (it appears once):

```rust
    pub repo_roots: Vec<String>,

```

Replace it with:

```rust
    pub repo_roots: Vec<String>,
    /// `owner/name` of the GitHub origin remote of the open panes (see
    /// `github_slug`). Panes without a GitHub remote add nothing.
    pub repo_remotes: Vec<String>,

```

Edit 3. Find this text exactly (it appears once):

```rust
    pub fn detect(notes_root: PathBuf, repo_roots: Vec<String>) -> Context {
```

Replace it with:

```rust
    pub fn detect(
        notes_root: PathBuf,
        repo_roots: Vec<String>,
        repo_remotes: Vec<String>,
    ) -> Context {
```

Edit 4. Find this text exactly (it appears once):

```rust
            repo_roots,
            screenshots: screenshots_dir,
```

Replace it with:

```rust
            repo_roots,
            repo_remotes,
            screenshots: screenshots_dir,
```

Edit 5. Find this text exactly (it appears once):

```rust
    /// `#1`, `#2` and so on when `${repo.root}` fans out over several repositories.
```

Replace it with:

```rust
    /// `#1`, `#2` and so on when `${repo.root}` or `${repo.remote}` fans out over several repositories.
```

Edit 6. Find this text exactly (it appears once):

```rust
    let fan_out = mentions(&value, REPO_ROOT);

    let roots: Vec<Option<String>> = if fan_out {
        let distinct: BTreeSet<&String> = ctx.repo_roots.iter().collect();
        if distinct.is_empty() {
            return Expanded::Skipped("no open pane is in a git repository".into());
        }
        distinct
            .into_iter()
            .map(|root| Some(root.clone()))
            .collect()
    } else {
        vec![None]
    };

    let mut rules = Vec::new();
    for (index, root) in roots.iter().enumerate() {
        match substitute(&value, ctx, root.as_deref()) {
```

Replace it with:

```rust
    let by_root = mentions(&value, REPO_ROOT);
    let by_remote = mentions(&value, REPO_REMOTE);
    if by_root && by_remote {
        return Expanded::Skipped("a rule cannot use both ${repo.root} and ${repo.remote}".into());
    }
    let fan_out = by_root || by_remote;

    // One entry per rule: the repository root, or the GitHub remote, or neither.
    let places: Vec<(Option<String>, Option<String>)> = if by_root {
        let distinct: BTreeSet<&String> = ctx.repo_roots.iter().collect();
        if distinct.is_empty() {
            return Expanded::Skipped("no open pane is in a git repository".into());
        }
        distinct
            .into_iter()
            .map(|root| (Some(root.clone()), None))
            .collect()
    } else if by_remote {
        let distinct: BTreeSet<&String> = ctx.repo_remotes.iter().collect();
        if distinct.is_empty() {
            return Expanded::Skipped("no open pane has a GitHub origin remote".into());
        }
        distinct
            .into_iter()
            .map(|remote| (None, Some(remote.clone())))
            .collect()
    } else {
        vec![(None, None)]
    };

    let mut rules = Vec::new();
    for (index, (root, remote)) in places.iter().enumerate() {
        match substitute(&value, ctx, root.as_deref(), remote.as_deref()) {
```

Edit 7. Find this text exactly (it appears once):

```rust
const REPO_ROOT: &str = "${repo.root}";
```

Replace it with:

```rust
const REPO_ROOT: &str = "${repo.root}";
const REPO_REMOTE: &str = "${repo.remote}";
```

Edit 8. Find this text exactly (it appears once):

```rust
fn substitute(value: &Value, ctx: &Context, repo: Option<&str>) -> Result<Value, String> {
    Ok(match value {
        Value::String(text) => Value::String(replace_variables(text, ctx, repo)?),
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|item| substitute(item, ctx, repo))
                .collect::<Result<_, _>>()?,
        ),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, item)| Ok((key.clone(), substitute(item, ctx, repo)?)))
```

Replace it with:

```rust
fn substitute(
    value: &Value,
    ctx: &Context,
    root: Option<&str>,
    remote: Option<&str>,
) -> Result<Value, String> {
    Ok(match value {
        Value::String(text) => Value::String(replace_variables(text, ctx, root, remote)?),
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|item| substitute(item, ctx, root, remote))
                .collect::<Result<_, _>>()?,
        ),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, item)| Ok((key.clone(), substitute(item, ctx, root, remote)?)))
```

Edit 9. Find this text exactly (it appears once):

```rust
fn replace_variables(text: &str, ctx: &Context, repo: Option<&str>) -> Result<String, String> {
```

Replace it with:

```rust
fn replace_variables(
    text: &str,
    ctx: &Context,
    root: Option<&str>,
    remote: Option<&str>,
) -> Result<String, String> {
```

Edit 10. Find this text exactly (it appears once):

```rust
            "repo.root" => repo
                .map(str::to_string)
                .ok_or("no open pane is in a git repository")?,
```

Replace it with:

```rust
            "repo.root" => root
                .map(str::to_string)
                .ok_or("no open pane is in a git repository")?,
            "repo.remote" => remote
                .map(str::to_string)
                .ok_or("no open pane has a GitHub origin remote")?,
```

Edit 11. Find this text exactly (it appears once):

```rust
            repo_roots: roots.iter().map(|r| r.to_string()).collect(),
            screenshots: shots,
```

Replace it with:

```rust
            repo_roots: roots.iter().map(|r| r.to_string()).collect(),
            repo_remotes: Vec::new(),
            screenshots: shots,
```

Edit 12. Find this text exactly (it appears once):

```rust
    #[test]
    fn an_unknown_variable_skips_the_rule() {
```

Replace it with:

```rust
    fn ctx_with_remotes(remotes: &[&str]) -> Context {
        let mut context = ctx(&[]);
        context.repo_remotes = remotes.iter().map(|r| r.to_string()).collect();
        context
    }

    #[test]
    fn repo_remote_makes_one_rule_per_distinct_remote() {
        let input = when(json!({ "repo": "${repo.remote}", "interval_s": 120 }));
        match expand(&input, &ctx_with_remotes(&["o/b", "o/a", "o/b"])) {
            Expanded::Rules(rules) => {
                assert_eq!(rules.len(), 2);
                assert_eq!(rules[0].0, "#1");
                assert_eq!(rules[0].1["repo"], json!("o/a"));
                assert_eq!(rules[0].1["interval_s"], json!(120));
                assert_eq!(rules[1].0, "#2");
                assert_eq!(rules[1].1["repo"], json!("o/b"));
            }
            Expanded::Skipped(reason) => panic!("skipped: {reason}"),
        }
    }

    #[test]
    fn one_remote_gives_one_rule_with_a_suffix() {
        let input = when(json!({ "repo": "${repo.remote}" }));
        match expand(&input, &ctx_with_remotes(&["o/a"])) {
            Expanded::Rules(rules) => {
                assert_eq!(rules.len(), 1);
                assert_eq!(rules[0].0, "#1");
            }
            Expanded::Skipped(reason) => panic!("skipped: {reason}"),
        }
    }

    #[test]
    fn repo_remote_without_a_github_remote_skips_the_rule() {
        let input = when(json!({ "repo": "${repo.remote}" }));
        // Open panes in repositories do not help: only roots are set here.
        assert_eq!(
            skipped(expand(&input, &ctx(&["/r/a"]))),
            "no open pane has a GitHub origin remote"
        );
    }

    #[test]
    fn repo_root_and_repo_remote_together_skip_the_rule() {
        let input = when(json!({ "repo": "${repo.remote}", "path": "${repo.root}/x" }));
        let mut both = ctx(&["/r/a"]);
        both.repo_remotes = vec!["o/a".into()];
        assert_eq!(
            skipped(expand(&input, &both)),
            "a rule cannot use both ${repo.root} and ${repo.remote}"
        );
    }

    #[test]
    fn an_unknown_variable_skips_the_rule() {
```

3. In `src-tauri/src/triggers/rules.rs`, inside `mod tests`, in the `ctx` helper, find

```rust
            repo_roots: roots.iter().map(|r| r.to_string()).collect(),
            screenshots: || None,
```

and replace it with

```rust
            repo_roots: roots.iter().map(|r| r.to_string()).collect(),
            repo_remotes: Vec::new(),
            screenshots: || None,
```

4. In the same file add this test directly before the test `a_rule_with_no_value_is_listed_as_skipped` (before its `#[test]` line):

```rust
    #[test]
    fn a_github_rule_gets_one_rule_per_remote() {
        let def = parse(
            "gh",
            r#"{"schemaVersion":1,"name":"Watch CI","triggers":[
                {"id":"ci","kind":"github.ci-failed","when":{"repo":"${repo.remote}"}}
            ]}"#,
        );
        let mut context = ctx(&[]);
        context.repo_remotes = vec!["o/b".into(), "o/a".into()];
        let plan = plan(&[def], &config(true, &["gh"]), &context);
        let ids: Vec<&str> = plan.rules.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, ["gh/ci#1", "gh/ci#2"]);
        assert_eq!(plan.rules[0].when["repo"], serde_json::json!("o/a"));
    }

```

5. In `src-tauri/src/triggers/mod.rs` make two edits.

   a. In `refresh_now`, find `    let ctx = Context::detect(state.notes.root(), roots);` and replace it with `    let ctx = Context::detect(state.notes.root(), roots, Vec::new());`

   b. In `mod tests`, in the `ctx` helper, find

```rust
            repo_roots: Vec::new(),
            screenshots: || None,
```

and replace it with

```rust
            repo_roots: Vec::new(),
            repo_remotes: Vec::new(),
            screenshots: || None,
```

6. Run `cd src-tauri && cargo fmt`. It should change nothing.

## Acceptance

- [ ] `cd src-tauri && cargo test -p terminal-grid repo_remote` passes (3 tests)
- [ ] `cd src-tauri && cargo test -p terminal-grid a_github_rule_gets_one_rule_per_remote` passes
- [ ] `cd src-tauri && cargo test -p terminal-grid triggers` passes (every earlier trigger test still passes)
- [ ] `cd src-tauri && cargo fmt --check` exits 0
- [ ] `cd src-tauri && cargo clippy -p terminal-grid --all-targets 2>&1 | grep -c warning` prints a number not larger than the baseline in the Log
- [ ] `git status --porcelain` lists only paths under `files:`, ignoring other card files under `plans/011-trigger-github/tasks/`

## Out of scope

- Reading the panes' remotes in `mod.rs` (T32). The empty `Vec::new()` stays until then.
- Documentation (T32).
- Changing how `${repo.root}` behaves, other than the shared parameter names.
- Fixing the app's existing clippy warnings.

## If stuck

- If a find text in step 2 or 3 is not found exactly, run `grep -n` for a distinctive line of it, make the equivalent edit on what you find, and note it in the Log. Do not rewrite other code.
- If the compiler reports a missing field `repo_remotes` in another place that builds a `Context`, add `repo_remotes: Vec::new(),` after its `repo_roots` line and note the file in the Log.
- If `cargo fmt` changes lines you wrote, keep its version.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed. Baseline clippy warnings 7, after 7. Step 2 Edit 2 find text had a trailing blank line that is not in variables.rs (line 164 is followed by a doc comment), so per the If stuck rule I inserted the repo_remotes field right after the repo_roots line without the extra blank line. Other find texts matched exactly. cargo fmt --check exit 0; repo_remote 3 passed; a_github_rule_gets_one_rule_per_remote 1 passed; triggers 48 passed.
- 2026-10-12 checker: pass
