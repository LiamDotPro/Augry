---
id: R05
epic: 001-trigger-core
question: Where do screenshots land by default on each OS, and how to read the configured folder on macOS (`com.apple.screencapture location`) and Windows
status: answered
---

## Answer

No new dependency is needed. The app already has `dirs = "7"` (src-tauri/Cargo.toml:27) for home and picture dirs.
macOS: run `defaults read com.apple.screencapture location`. Exit 0 prints the path, so expand a leading `~`. Any failure means `~/Desktop`.
Windows: run `reg query "HKCU\...\User Shell Folders" /v {B7BEDE81-DF94-4682-A7D8-57A52620B86F}` and expand `%VAR%`. Fall back to `dirs::picture_dir()/Screenshots`.
Linux: `dirs::picture_dir()/Screenshots` if that directory exists, else `dirs::picture_dir()` if it exists. No Linux tool has a readable "screenshot folder" setting that is worth parsing.
Return `None` (rule skipped) only when the final candidate directory does not exist or no home or picture dir is known.

## Evidence

Ran on this Mac (Darwin 25.2.0, 2026-10-12):
- `defaults read com.apple.screencapture location` printed `~/Documents/` with exit code 0. The tilde is NOT expanded and there is a trailing slash.
- `defaults read com.apple.screencapture location-nonexist` printed "The domain/default pair ... does not exist" to stderr, exit code 1. A machine that never changed the setting behaves the same for `location`, and macOS then saves to `~/Desktop`.
- The same plist also holds `target = file` (other values: `clipboard`, `preview`, `mail` and so on; with `clipboard` no file is written) and `location-last`. Ignore both. `location` is the live setting.
- `~/Desktop` exists on this machine.

Repo:
- src-tauri/Cargo.toml:27 `dirs = "7"`; Cargo.lock has dirs 6.0.0 and 7.0.0. dirs 7.0.0 source (`~/.cargo/registry/src/*/dirs-7.0.0/src/lib.rs`) has `picture_dir()` (line 262) and `desktop_dir()` (line 214), but no screenshots function.
- Existing uses: `dirs::home_dir()` in src-tauri/src/pty/mod.rs:493 (wrapped as `crate::pty::home_dir()`), `dirs::document_dir()` in src-tauri/src/notes/mod.rs:79.
- `windows-sys`, `windows`, `winreg` and `plist` are in Cargo.lock only as transitive dependencies. They are not direct dependencies, so using them would be a new dependency under Decision 18. Do not use them.
- src-tauri/src/triggers/ does not exist yet.

Windows, read 2026-10-12 (secondary sources, no Microsoft page lists it directly):
- FOLDERID_Screenshots = {B7BEDE81-DF94-4682-A7D8-57A52620B86F} (https://winshl-kb.readthedocs.io/en/latest/sources/known-folders/b7bede81-df94-4682-a7d8-57a52620b86f.html, https://freepascal.org/daily/doc/rtl/windirs/folderid_screenshots.html).
- Per user value: `HKCU\SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\User Shell Folders`, value name is the GUID, type REG_EXPAND_SZ, for example `D:\Users\x\Pictures\Screenshots` or `%USERPROFILE%\Pictures\Screenshots` (https://www.elevenforum.com/t/user-folder-displaying-wrong-name.44921/latest). Win+PrtSc saves there on Windows 10 and 11. Win+Shift+S (Snipping Tool) goes to the clipboard, and Snipping Tool saves to the same folder only if the user turns that on.
- Not run: this is a Mac, so the Windows command is unverified.

Linux (from knowledge, not re-fetched): GNOME 42 and later (built-in screenshot UI, PrtSc) saves to `~/Pictures/Screenshots`, older GNOME to `~/Pictures`. KDE Spectacle saves to `~/Pictures/Screenshots` by default in current versions, older ones to `~/Pictures`. Both honor `XDG_PICTURES_DIR` from `~/.config/user-dirs.dirs`, which `dirs::picture_dir()` reads.

## Implications for the cards

Put one function in `src-tauri/src/triggers/variables.rs`:
`fn screenshots_dir() -> Option<PathBuf>` with three `#[cfg(target_os = ...)]` bodies. Return `Some(path)` only if `path.is_dir()`, else `None` and the rule is skipped with a logged reason.

macOS body:
1. `Command::new("defaults").args(["read", "com.apple.screencapture", "location"]).output()`.
2. If the exit status is success, take stdout as UTF-8, `trim()` it (trailing newline), and trim trailing `/` unless the string is just `/`. If it is empty, treat as unset.
3. Expand the tilde: `~` alone or a `~/` prefix becomes `crate::pty::home_dir()` joined with the rest. Do not handle `~user`. If the value is not absolute after that, treat it as unset.
4. If unset or the command failed, use `crate::pty::home_dir().join("Desktop")`.
5. If the user set a location that does not exist, return `None`. Do not fall back to Desktop in that case, because screenshots would not land there. (macOS recreates nothing; it falls back to Desktop itself, so it is acceptable to try Desktop here too. Pick one and test it; the first is the safer reading.)

Windows body:
1. `Command::new("reg").args(["query", r"HKCU\SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\User Shell Folders", "/v", "{B7BEDE81-DF94-4682-A7D8-57A52620B86F}"])`. Set `creation_flags(0x08000000)` (CREATE_NO_WINDOW) via `std::os::windows::process::CommandExt` so no console flashes.
2. Parse stdout: find the line containing `REG_EXPAND_SZ` (or `REG_SZ`), take everything after that token, trim.
3. Expand `%NAME%` segments with `std::env::var(NAME)`. If a variable is missing, treat as unset.
4. If the value is missing or the command fails, use `dirs::picture_dir()?.join("Screenshots")`.
5. Do not use `dirs::picture_dir()` alone as the main answer; it is the Pictures folder, not Screenshots.

Linux body (and any other unix): `let p = dirs::picture_dir()?; let s = p.join("Screenshots"); if s.is_dir() { Some(s) } else if p.is_dir() { Some(p) } else { None }`.

Tests: split the pure parts (tilde expansion, `reg query` output parsing, `%VAR%` expansion) into plain functions taking strings so they run on any OS with `cargo test`. Sample reg output to test: `    {B7BEDE81-DF94-4682-A7D8-57A52620B86F}    REG_EXPAND_SZ    %USERPROFILE%\Pictures\Screenshots`.

## Doubts

- Windows command and output format were not run here. The parse (value after the `REG_EXPAND_SZ` token) matches documented `reg query` output but should be checked on a Windows machine.
- The value can be absent on a stock Windows install; the fallback covers it.
- Linux defaults were not re-verified against current GNOME and KDE docs. The `is_dir` fallback chain makes this safe either way.
- The macOS `target` setting (clipboard) means no file appears even when the folder exists. The rule simply never fires; no handling needed.
