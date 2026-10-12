---
id: R02
epic: 001-trigger-core
question: Current `globset` version; does `**/*.md` match `docs\a.md` on Windows, or must paths be normalised to `/` first?
status: answered
---

## Answer

Use `globset = "0.4.20"` (current on crates.io, 2026-10-12). On Windows, `Candidate::new` (used by `GlobSet::is_match`) rewrites every path separator to `/` itself, so `**/*.md` matches `docs\a.md` without manual normalisation. Build every glob with `literal_separator(true)`. Feed the matcher the path relative to `when.path`. Write directory ignores as `**/name/**`.

## Evidence

- `cargo search globset` printed `globset = "0.4.20"`; `cargo add` in a scratch project resolved v0.4.20.
- Source `globset-0.4.20/src/pathutil.rs` lines 56 to 77: `normalize_path` is a no-op under `cfg(unix)` and, under `cfg(not(unix))`, replaces every byte for which `std::path::is_separator` is true with `/`. `Candidate::new` calls it. So on Windows `\` is converted; on unix `\` is an ordinary filename character. I could not run on Windows (this is macOS), so the Windows claim comes from the source, not a run.
- Ran in a scratch crate on macOS, all with `GlobBuilder::new(p).literal_separator(true)`:
  - `**/*.md` matched `a/b.md` and `top.md` (true, true).
  - `*.md` vs `a/b.md`: false with literal_separator, true without. So the option matters.
  - `**/*` matched `x` and `.hidden` (dotfiles are not excluded).
  - `**/*.MD` with `case_insensitive(true)` matched `a/b.md`.
  - `**/*.md` matched the absolute `/Users/x/a.md`, so an unrelativised path also matches, but relative is still the plan.
  - Ignore set `**/.git/**`, `**/node_modules/**`, `**/target/**`, `**/.DS_Store`, `**/*.crdownload`, `**/*.part`, `**/*.download`, `**/*.tmp` (built case sensitive):
    - true: `.git/config`, `a/.git/HEAD`, `node_modules/x/y.js`, `a/b/node_modules/x`, `target/debug/a`, `src/target/x`, `.DS_Store`, `a/.DS_Store`, `f.crdownload`, `a/f.tmp`
    - false: `a/targetx/f`, `.gitignore` (good); `.git` and `a/.git` themselves (the bare directory); `a/f.PART` (case).

## Implications for the cards

- Workspace: `globset = "0.4.20"` in `[workspace.dependencies]`; `trigger-fs` uses `globset = { workspace = true }`.
- Helper in `trigger-fs` (pure, in `logic.rs`): 
  `GlobBuilder::new(p).literal_separator(true).case_insensitive(true).build()?`, add to a `GlobSetBuilder`, then `.build()?`. Use `case_insensitive(true)` for the default ignores so `.PART` and `.CRDOWNLOAD` match. For user `glob` use the same builder (macOS and Windows are case insensitive filesystems); state the choice in the card.
- Default ignore patterns, exactly: `**/.git`, `**/.git/**`, `**/node_modules`, `**/node_modules/**`, `**/target`, `**/target/**`, `**/.DS_Store`, `**/*.crdownload`, `**/*.part`, `**/*.download`, `**/*.tmp`. The bare `**/name` forms are needed so events on the directory itself are ignored. User `ignore` globs are appended to this list.
- Matching input: `path.strip_prefix(when.path)` then pass the relative `&Path` to `GlobSet::is_match`. Skip the event if `strip_prefix` fails. Do not convert to a `String` with replace; on Windows globset normalises, on unix backslash must stay a literal.
- A trailing-slash directory ignore like `.git/` is not valid syntax for globset; do not pass the epic's wording directly, use the patterns above.
- Compile the sets once when the rule is armed, not per event. A bad user glob returns `globset::Error`; surface it as a skipped rule with a reason.
- Add a unit test with `docs/a.md`, `a/.git/HEAD`, `a/targetx/f`, `x.PART` so unix CI covers the behaviour; add a `#[cfg(windows)]` test with `Path::new(r"docs\a.md")` for `**/*.md`.

## Doubts

- Windows behaviour not executed here, only read from source. A Windows CI run would confirm.
- Case sensitivity of user `glob` is a design choice for the card, not a globset fact.
