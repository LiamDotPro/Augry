---
id: T11
epic: 002-trigger-internal
title: Load the bash integration through a wrapper rcfile
status: todo
depends_on: [T10]
size: S
model: haiku
files:
  - src-tauri/resources/bashrc
  - src-tauri/src/pty/shell.rs
  - src-tauri/src/pty/shell/live.rs
---

## Goal

bash panes run the shell integration: bash is launched as `bash --rcfile <app data>/bashrc -i`, and that file runs the user's own `~/.bashrc` when it exists, then the integration script. Today the launch args `-c '. script; exec bash -i'` throw the script away (research R01), so no bash pane ever reported a command end. A pty test starts the real bash with the real args.

## Read first

- `plans/002-trigger-internal/epic.md` Decision 7 (bash)
- `src-tauri/src/pty/shell.rs` the whole file as T10 left it: `integration_args`, `WRAPPERS`, `resolve_in`
- `src-tauri/src/pty/shell/live.rs` the test harness T10 added (`Sandbox`, `run`, `find_shell`)
- `src-tauri/resources/shell-integration.sh` the script the wrapper sources (not changed here)

## Steps

1. Before changing anything, run `cd src-tauri && cargo clippy -p terminal-grid --all-targets 2>&1 | grep -c warning` and write the number in the Log. It is the baseline for the clippy check in Acceptance.

2. Create `src-tauri/resources/bashrc` (no extension) with exactly this content:

```bash
# Terminal Grid bash wrapper. Launched as `bash --rcfile <this file> -i`, which would
# skip the user's own ~/.bashrc, so it runs that first and then the integration.
if [ -f "$HOME/.bashrc" ]; then . "$HOME/.bashrc"; fi
. "$(dirname "${BASH_SOURCE[0]}")/shell-integration.sh"
```

3. Edit `src-tauri/src/pty/shell.rs`. Use exact find and replace, in this order.

3a. Delete the function `is_posix` from `impl ShellKind` (nothing uses it after step 3d). Delete the blank line above it and these three lines:

```rust
    fn is_posix(self) -> bool {
        matches!(self, ShellKind::Bash | ShellKind::Zsh)
    }
```

3b. In `resolve_in`, find:

```rust
    let mut args = integration_args(kind, script_dir);
```

Replace with:

```rust
    // `sh` is classed as bash for the frontend's sake, but it may be dash,
    // which has no --rcfile, so it starts bare.
    let mut args = if is_plain_sh(&program) {
        Vec::new()
    } else {
        integration_args(kind, script_dir)
    };
```

3c. Directly above the line `fn integration_args(kind: ShellKind, script_dir: &Path) -> Vec<String> {` insert:

```rust
fn is_plain_sh(program: &str) -> bool {
    Path::new(program)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .is_some_and(|stem| stem.eq_ignore_ascii_case("sh"))
}

```

3d. In `integration_args`, find this whole block:

```rust
    if kind.is_posix() {
        let script = script_dir.join("shell-integration.sh");
        // --rcfile would skip the user's own rc, so source ours afterwards instead.
        return vec![
            "-c".to_string(),
            format!(
                ". \"{}\"; exec \"$0\" -i",
                script.display().to_string().replace('"', "\\\"")
            ),
        ];
    }
```

Replace with:

```rust
    if kind == ShellKind::Bash {
        // A wrapper rc file runs the user's own ~/.bashrc, then the integration.
        // `-i` is needed because --rcfile is only read by interactive shells.
        return vec![
            "--rcfile".to_string(),
            script_dir.join("bashrc").display().to_string(),
            "-i".to_string(),
        ];
    }
```

3e. Install the new file. Find `const WRAPPERS: [(&str, &str); 4] = [` and change `4` to `5`. Then find this line:

```rust
    ("zsh/.zlogin", include_str!("../../resources/zsh/.zlogin")),
];
```

Replace with:

```rust
    ("zsh/.zlogin", include_str!("../../resources/zsh/.zlogin")),
    ("bashrc", include_str!("../../resources/bashrc")),
];
```

3f. In `mod tests`, directly above `    #[test]` + `    fn cmd_launches_bare() {`, insert:

```rust
    #[test]
    fn bash_gets_the_wrapper_rcfile_and_sh_starts_bare() {
        let args = integration_args(ShellKind::Bash, Path::new("/data/tg"));
        assert_eq!(args, vec!["--rcfile", "/data/tg/bashrc", "-i"]);
        let shell = resolve_in(
            Some("/bin/sh"),
            &[],
            Path::new("/data/tg"),
            None,
            Path::new("/h"),
        );
        assert_eq!(shell.kind, ShellKind::Bash);
        assert!(shell.args.is_empty());
        let shell = resolve_in(
            Some("/bin/bash"),
            &[],
            Path::new("/data/tg"),
            None,
            Path::new("/h"),
        );
        assert_eq!(shell.args.len(), 3);
    }

    #[test]
    fn install_writes_the_bash_wrapper() {
        let root = std::env::temp_dir().join(format!("tg-wrappers-{}", uuid::Uuid::new_v4()));
        install_scripts(&root, &root.join("data")).unwrap();
        let text = fs::read_to_string(root.join("data/bashrc")).unwrap();
        assert!(text.contains("shell-integration.sh"));
        fs::remove_dir_all(&root).unwrap();
    }

```

4. Append these two tests to the end of `src-tauri/src/pty/shell/live.rs`:

```rust

#[test]
fn bash_reports_command_ends_and_runs_the_users_bashrc() {
    let Some(bash) = find_shell("bash") else {
        eprintln!("skipping: bash is not installed");
        return;
    };
    let sandbox = Sandbox::new();
    sandbox.write_home(".bashrc", "export TG_RC_MARKER=from-bashrc\n");
    let out = run(
        &bash,
        &sandbox,
        "false\nprintf 'rc=%s\\n' \"$TG_RC_MARKER\"\nexit\n",
    );
    assert!(
        out.contains("\x1b]133;D;1\x07"),
        "no OSC 133;D;1 in {out:?}"
    );
    assert!(
        out.contains("rc=from-bashrc"),
        "~/.bashrc did not run: {out:?}"
    );
}

#[test]
fn bash_works_without_a_bashrc() {
    let Some(bash) = find_shell("bash") else {
        eprintln!("skipping: bash is not installed");
        return;
    };
    let sandbox = Sandbox::new();
    let out = run(&bash, &sandbox, "false\nexit\n");
    assert!(
        out.contains("\x1b]133;D;1\x07"),
        "no OSC 133;D;1 in {out:?}"
    );
}
```

5. Run `cd src-tauri && cargo fmt`. If it changes the code above, keep the result.

## Acceptance

- [ ] `cd src-tauri && cargo test -p terminal-grid pty::shell` passes and lists `bash_reports_command_ends_and_runs_the_users_bashrc`, `bash_works_without_a_bashrc`, `bash_gets_the_wrapper_rcfile_and_sh_starts_bare` and `install_writes_the_bash_wrapper` as ok
- [ ] `cd src-tauri && cargo test -p terminal-grid pty::shell::live -- --nocapture 2>&1 | grep -c skipping` prints `0` on a machine that has bash and zsh
- [ ] `grep -c "is_posix" src-tauri/src/pty/shell.rs` prints `0`
- [ ] `cd src-tauri && cargo test --workspace` passes
- [ ] `cd src-tauri && cargo fmt --check` exits 0
- [ ] `cd src-tauri && cargo check -p terminal-grid --no-default-features` exits 0
- [ ] `cd src-tauri && cargo clippy -p terminal-grid --all-targets 2>&1 | grep -c warning` prints the same number as the baseline in the Log, or a lower one
- [ ] `git status --porcelain | grep -v 'plans/002-trigger-internal/tasks/'` lists only paths under `files:`

## Out of scope

- The `$HISTCMD` change in `shell-integration.sh` (T12) and PowerShell (T13).
- Changing `ShellKind::from_program`, so `sh` stays classed as bash. Only its launch args are dropped.
- Reading `~/.bash_profile` or `~/.profile`. The old launch did not either.
- Changing `tauri.conf.json`, docs, or the frontend.
- Fixing existing clippy warnings or renaming anything.

## If stuck

- If `cargo test` says `couldn't read ../../resources/bashrc`, step 2 was missed. The file has no extension.
- If the build says `no method named is_posix`, step 3d was missed. If clippy says `is_posix` is never used, step 3a was missed.
- If a live bash test fails with `no OSC 133;D;1`, copy the printed output into the Log and set `status: blocked`. Do not change the assertion.
- If `cargo fmt` changes code given above, keep the formatted result; that is expected.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
