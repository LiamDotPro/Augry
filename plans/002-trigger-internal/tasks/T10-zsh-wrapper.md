---
id: T10
epic: 002-trigger-internal
title: Load the zsh integration through a ZDOTDIR wrapper
status: done
depends_on: [T09]
size: M
model: sonnet
files:
  - src-tauri/resources/zsh/.zshenv
  - src-tauri/resources/zsh/.zprofile
  - src-tauri/resources/zsh/.zshrc
  - src-tauri/resources/zsh/.zlogin
  - src-tauri/src/pty/shell.rs
  - src-tauri/src/pty/shell/live.rs
  - src-tauri/src/pty/mod.rs
---

## Goal

zsh panes run the shell integration for the first time: it emits `OSC 133;D` after each command, and the user's own `.zshenv`, `.zshrc` and the rest still run. Today the launch args `-c '. script; exec zsh -i'` throw the script away (research R01), so no zsh pane ever reported anything. The card is `model: sonnet` because the wrapper files interact with the user's real dotfiles and `/etc/zshrc`; if a test fails on the builder's machine, working out which zsh startup step misbehaved needs judgement. The files below are already proven on zsh 5.9 (macOS).

## Read first

- `plans/002-trigger-internal/epic.md` Decision 7 (zsh)
- `plans/002-trigger-internal/research/R01-feeds-and-shell-integration.md` the "Bug found by running it" bullet
- `src-tauri/src/pty/shell.rs` the whole file (243 lines): `Shell`, `resolve`, `integration_args`, `install_scripts`, the tests
- `src-tauri/src/pty/mod.rs:205-225` where `shell.args` are put on the command
- `src-tauri/resources/shell-integration.sh` the script the wrapper sources (not changed here)

## Steps

1. Before changing anything, run `cd src-tauri && cargo clippy -p terminal-grid --all-targets 2>&1 | grep -c warning` and write the number in the Log. It is the baseline for the clippy check in Acceptance.

2. Create the four wrapper files in `src-tauri/resources/zsh/` with exactly this content. The names start with a dot. Each one runs the user's own file of the same name with `ZDOTDIR` pointing at the user's folder, then puts `ZDOTDIR` back.

`src-tauri/resources/zsh/.zshenv`:

```zsh
# Terminal Grid ZDOTDIR wrapper, part 1 of 4. See src/pty/shell.rs.
# Runs the user's own .zshenv with ZDOTDIR pointing at their folder, then puts ours back.
if [[ -n "$TG_USER_ZDOTDIR" && -r "$TG_USER_ZDOTDIR/.zshenv" ]]; then
  TG_WRAPPER_ZDOTDIR=$ZDOTDIR
  ZDOTDIR=$TG_USER_ZDOTDIR
  . "$TG_USER_ZDOTDIR/.zshenv"
  # The user's file may have moved ZDOTDIR; that is where their other files are.
  TG_USER_ZDOTDIR=$ZDOTDIR
  ZDOTDIR=$TG_WRAPPER_ZDOTDIR
  unset TG_WRAPPER_ZDOTDIR
fi
```

`src-tauri/resources/zsh/.zprofile`:

```zsh
# Terminal Grid ZDOTDIR wrapper, part 2 of 4. See src/pty/shell.rs.
# Runs the user's own .zprofile with ZDOTDIR pointing at their folder, then puts ours back.
if [[ -n "$TG_USER_ZDOTDIR" && -r "$TG_USER_ZDOTDIR/.zprofile" ]]; then
  TG_WRAPPER_ZDOTDIR=$ZDOTDIR
  ZDOTDIR=$TG_USER_ZDOTDIR
  . "$TG_USER_ZDOTDIR/.zprofile"
  # The user's file may have moved ZDOTDIR; that is where their other files are.
  TG_USER_ZDOTDIR=$ZDOTDIR
  ZDOTDIR=$TG_WRAPPER_ZDOTDIR
  unset TG_WRAPPER_ZDOTDIR
fi
```

`src-tauri/resources/zsh/.zshrc`:

```zsh
# Terminal Grid ZDOTDIR wrapper, part 3 of 4. See src/pty/shell.rs.
# Runs the user's own .zshrc, then the shell integration.
TG_WRAPPER_ZDOTDIR=$ZDOTDIR
# /etc/zshrc on macOS puts the history file under ZDOTDIR, which is ours at that point.
if [[ "$HISTFILE" == "$TG_WRAPPER_ZDOTDIR/.zsh_history" ]]; then
  HISTFILE="$TG_USER_ZDOTDIR/.zsh_history"
fi
if [[ -n "$TG_USER_ZDOTDIR" && -r "$TG_USER_ZDOTDIR/.zshrc" ]]; then
  ZDOTDIR=$TG_USER_ZDOTDIR
  . "$TG_USER_ZDOTDIR/.zshrc"
  TG_USER_ZDOTDIR=$ZDOTDIR
  ZDOTDIR=$TG_WRAPPER_ZDOTDIR
fi
[[ -r "$TG_SHELL_INTEGRATION" ]] && . "$TG_SHELL_INTEGRATION"
# A login shell still reads our .zlogin next, which restores ZDOTDIR for good.
if [[ ! -o login ]]; then
  ZDOTDIR=$TG_USER_ZDOTDIR
  unset TG_USER_ZDOTDIR TG_WRAPPER_ZDOTDIR TG_SHELL_INTEGRATION
fi
```

`src-tauri/resources/zsh/.zlogin`:

```zsh
# Terminal Grid ZDOTDIR wrapper, part 4 of 4. See src/pty/shell.rs.
# Runs the user's own .zlogin, then hands ZDOTDIR back to the user for good.
if [[ -n "$TG_USER_ZDOTDIR" && -r "$TG_USER_ZDOTDIR/.zlogin" ]]; then
  ZDOTDIR=$TG_USER_ZDOTDIR
  . "$TG_USER_ZDOTDIR/.zlogin"
  TG_USER_ZDOTDIR=$ZDOTDIR
fi
ZDOTDIR=$TG_USER_ZDOTDIR
unset TG_USER_ZDOTDIR TG_WRAPPER_ZDOTDIR TG_SHELL_INTEGRATION
```

3. Edit `src-tauri/src/pty/shell.rs`. Use exact find and replace. Make the edits in this order.

3a. Add a field to `Shell`. Find:

```rust
pub struct Shell {
    pub program: String,
    pub kind: ShellKind,
    pub args: Vec<String>,
}
```

Replace with:

```rust
pub struct Shell {
    pub program: String,
    pub kind: ShellKind,
    pub args: Vec<String>,
    /// Variables the integration needs, set on the child on top of the app's own.
    pub env: Vec<(String, String)>,
}
```

3b. Split `resolve`. Find this one line (the doc comment above it stays where it is):

```rust
pub fn resolve(configured: Option<&str>, extra_args: &[String], script_dir: &Path) -> Shell {
```

Replace it with:

```rust
pub fn resolve(configured: Option<&str>, extra_args: &[String], script_dir: &Path) -> Shell {
    resolve_in(
        configured,
        extra_args,
        script_dir,
        std::env::var("ZDOTDIR").ok().as_deref(),
        &super::home_dir(),
    )
}

/// `resolve` with the user's `ZDOTDIR` and home folder given, so tests can use a
/// fake home.
fn resolve_in(
    configured: Option<&str>,
    extra_args: &[String],
    script_dir: &Path,
    user_zdotdir: Option<&str>,
    home: &Path,
) -> Shell {
```

3c. Still in `resolve_in`, find:

```rust
    Shell {
        program,
        kind,
        args,
    }
```

Replace with:

```rust
    Shell {
        program,
        kind,
        args,
        env: integration_env(kind, script_dir, user_zdotdir, home),
    }
```

3d. In `integration_args`, find:

```rust
    if kind.is_posix() {
        let script = script_dir.join("shell-integration.sh");
```

Replace with (the zsh branch is new, the two old lines stay after it):

```rust
    if kind == ShellKind::Zsh {
        // The wrapper dotfiles in `ZDOTDIR` (see `integration_env`) load the
        // integration, so zsh just starts interactive. The old `-c '. script;
        // exec zsh -i'` lost the script's functions in the `exec`.
        return vec!["-i".to_string()];
    }
    if kind.is_posix() {
        let script = script_dir.join("shell-integration.sh");
```

3e. Directly above the doc comment line `/// Copies the bundled integration scripts into the app data dir on first run.` insert:

```rust
/// What zsh needs to find the wrapper dotfiles: `ZDOTDIR` points at our `zsh`
/// folder, and the user's real one is kept in `TG_USER_ZDOTDIR` so each wrapper
/// can run the user's own file and put things back. Other shells need nothing.
fn integration_env(
    kind: ShellKind,
    script_dir: &Path,
    user_zdotdir: Option<&str>,
    home: &Path,
) -> Vec<(String, String)> {
    if kind != ShellKind::Zsh {
        return Vec::new();
    }
    let user = user_zdotdir
        .filter(|dir| !dir.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| home.display().to_string());
    vec![
        (
            "ZDOTDIR".to_string(),
            script_dir.join("zsh").display().to_string(),
        ),
        ("TG_USER_ZDOTDIR".to_string(), user),
        (
            "TG_SHELL_INTEGRATION".to_string(),
            script_dir
                .join("shell-integration.sh")
                .display()
                .to_string(),
        ),
    ]
}

/// Files the shells read in place of the user's own rc files. They are compiled
/// in, so installing them does not depend on how the bundle lays out folders.
const WRAPPERS: [(&str, &str); 4] = [
    ("zsh/.zshenv", include_str!("../../resources/zsh/.zshenv")),
    (
        "zsh/.zprofile",
        include_str!("../../resources/zsh/.zprofile"),
    ),
    ("zsh/.zshrc", include_str!("../../resources/zsh/.zshrc")),
    ("zsh/.zlogin", include_str!("../../resources/zsh/.zlogin")),
];

```

3f. In `install_scripts`, find:

```rust
            let target = target_dir.join(name);
            let contents = fs::read(&source)?;
            match fs::remove_file(&target) {
                Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(error),
                _ => {}
            }
            fs::write(&target, contents)?;
        }
    }
    Ok(())
}
```

Replace with:

```rust
            write_fresh(&target_dir.join(name), &fs::read(&source)?)?;
        }
    }
    for (name, contents) in WRAPPERS {
        let target = target_dir.join(name);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        write_fresh(&target, contents.as_bytes())?;
    }
    Ok(())
}

/// Removes the old file first, then writes. See `install_scripts` for why.
fn write_fresh(target: &Path, contents: &[u8]) -> std::io::Result<()> {
    match fs::remove_file(target) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(error),
        _ => {}
    }
    fs::write(target, contents)
}
```

3g. Directly above the line `#[cfg(test)]` that is followed by `mod tests {`, insert:

```rust
#[cfg(all(test, unix))]
mod live;

```

3h. In `mod tests`, directly above `    #[test]` + `    fn cmd_launches_bare() {`, insert:

```rust
    #[test]
    fn zsh_starts_interactive_with_the_wrapper_folder() {
        let script_dir = Path::new("/data/tg");
        assert_eq!(integration_args(ShellKind::Zsh, script_dir), vec!["-i"]);
        let env = integration_env(ShellKind::Zsh, script_dir, None, Path::new("/home/ann"));
        let get = |key: &str| env.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str());
        assert_eq!(get("ZDOTDIR"), Some("/data/tg/zsh"));
        assert_eq!(get("TG_USER_ZDOTDIR"), Some("/home/ann"));
        assert_eq!(
            get("TG_SHELL_INTEGRATION"),
            Some("/data/tg/shell-integration.sh")
        );
        let moved = integration_env(
            ShellKind::Zsh,
            script_dir,
            Some("/cfg/zsh"),
            Path::new("/h"),
        );
        assert!(moved.contains(&("TG_USER_ZDOTDIR".to_string(), "/cfg/zsh".to_string())));
        assert!(integration_env(ShellKind::Bash, script_dir, None, Path::new("/h")).is_empty());
    }

    #[test]
    fn install_writes_the_zsh_wrappers() {
        let root = std::env::temp_dir().join(format!("tg-wrappers-{}", uuid::Uuid::new_v4()));
        install_scripts(&root, &root.join("data")).unwrap();
        for name in [".zshenv", ".zprofile", ".zshrc", ".zlogin"] {
            let text = fs::read_to_string(root.join("data/zsh").join(name)).unwrap();
            assert!(text.contains("TG_USER_ZDOTDIR"), "{name}");
        }
        fs::remove_dir_all(&root).unwrap();
    }

```

4. In `src-tauri/src/pty/mod.rs`, in `PtyManager::spawn`, find:

```rust
        for arg in &shell.args {
            command.arg(arg);
        }
```

Replace with:

```rust
        for arg in &shell.args {
            command.arg(arg);
        }
        for (key, value) in &shell.env {
            command.env(key, value);
        }
```

5. Create `src-tauri/src/pty/shell/live.rs` (make the folder `src-tauri/src/pty/shell/` first). It starts the real zsh with the real args and variables from `resolve_in`, under a pty, in a fake home. A shell that is not installed makes its test skip. Content:

```rust
//! Tests that start the real shell with the launch arguments and variables the
//! app uses, under a pty, in a fake home folder. A shell that is not installed
//! makes its test skip, so `cargo test` passes on any machine.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use portable_pty::{native_pty_system, CommandBuilder, PtySize};

use super::*;

/// A fake home and an app data folder with a space in its name, like the
/// "Application Support" folder on macOS. Removed when dropped.
struct Sandbox {
    root: PathBuf,
    home: PathBuf,
    script_dir: PathBuf,
}

impl Sandbox {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("tg-live-{}", uuid::Uuid::new_v4()));
        let home = root.join("home");
        let script_dir = root.join("app data");
        fs::create_dir_all(&home).unwrap();
        // The crate folder holds `resources/`, which is what the bundle's
        // resource dir looks like to `install_scripts`.
        install_scripts(Path::new(env!("CARGO_MANIFEST_DIR")), &script_dir).unwrap();
        Sandbox {
            root,
            home,
            script_dir,
        }
    }

    /// Writes a file in the fake home, creating its folders first.
    fn write_home(&self, name: &str, text: &str) {
        let path = self.home.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn find_shell(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
}

/// Starts `shell` the way `PtyManager::spawn` does (arguments and variables
/// from `resolve_in`), types `input` and returns everything it printed. The
/// input must end with `exit`. A shell that is still running after 20 seconds
/// is killed and what it printed so far is returned.
fn run(shell: &Path, sandbox: &Sandbox, input: &str) -> String {
    let resolved = resolve_in(
        shell.to_str(),
        &[],
        &sandbox.script_dir,
        None,
        &sandbox.home,
    );
    let mut command = CommandBuilder::new(&resolved.program);
    command.args(&resolved.args);
    command.cwd(&sandbox.home);
    command.env("HOME", &sandbox.home);
    command.env("TERM", "xterm-256color");
    command.env_remove("ZDOTDIR");
    // A developer's own history settings would change what counts as a new command.
    command.env_remove("HISTCONTROL");
    command.env_remove("HISTIGNORE");
    for (key, value) in &resolved.env {
        command.env(key, value);
    }

    let pair = native_pty_system()
        .openpty(PtySize {
            rows: 24,
            cols: 200,
            pixel_width: 0,
            pixel_height: 0,
        })
        .unwrap();
    let mut child = pair.slave.spawn_command(command).unwrap();
    drop(pair.slave);
    let mut reader = pair.master.try_clone_reader().unwrap();
    let mut writer = pair.master.take_writer().unwrap();

    let collector = std::thread::spawn(move || {
        let mut out = Vec::new();
        let mut buffer = [0u8; 4096];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(count) => out.extend_from_slice(&buffer[..count]),
            }
        }
        String::from_utf8_lossy(&out).into_owned()
    });

    writer.write_all(input.as_bytes()).unwrap();
    writer.flush().unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline && child.try_wait().unwrap().is_none() {
        std::thread::sleep(Duration::from_millis(50));
    }
    let _ = child.kill();
    drop(writer);
    drop(pair.master);
    collector.join().unwrap()
}

#[test]
fn zsh_reports_command_ends_and_runs_the_users_files() {
    let Some(zsh) = find_shell("zsh") else {
        eprintln!("skipping: zsh is not installed");
        return;
    };
    let sandbox = Sandbox::new();
    sandbox.write_home(".zshenv", "export TG_ENV_MARKER=from-zshenv\n");
    sandbox.write_home(".zshrc", "export TG_RC_MARKER=from-zshrc\n");
    let out = run(
        &zsh,
        &sandbox,
        "false\nprintf 'rc=%s env=%s hist=%s\\n' \"$TG_RC_MARKER\" \"$TG_ENV_MARKER\" \"$HISTFILE\"\nexit\n",
    );
    assert!(
        out.contains("\x1b]133;D;1\x07"),
        "no OSC 133;D;1 in {out:?}"
    );
    assert!(
        out.contains("rc=from-zshrc env=from-zshenv"),
        "the user's files did not run: {out:?}"
    );
    // macOS's /etc/zshrc puts the history file under ZDOTDIR.
    assert!(!out.contains("app data"), "ZDOTDIR leaked: {out:?}");
}

#[test]
fn zsh_follows_a_zdotdir_that_the_users_zshenv_sets() {
    let Some(zsh) = find_shell("zsh") else {
        eprintln!("skipping: zsh is not installed");
        return;
    };
    let sandbox = Sandbox::new();
    sandbox.write_home(".zshenv", "export ZDOTDIR=\"$HOME/dots\"\n");
    sandbox.write_home("dots/.zshrc", "export TG_RC_MARKER=from-dots\n");
    let out = run(
        &zsh,
        &sandbox,
        "false\nprintf 'rc=%s\\n' \"$TG_RC_MARKER\"\nexit\n",
    );
    assert!(
        out.contains("\x1b]133;D;1\x07"),
        "no OSC 133;D;1 in {out:?}"
    );
    assert!(
        out.contains("rc=from-dots"),
        "dots/.zshrc did not run: {out:?}"
    );
}
```

6. Run `cd src-tauri && cargo fmt`. If it changes the code above, keep the result.

## Acceptance

- [ ] `cd src-tauri && cargo test -p terminal-grid pty::shell` passes and lists `zsh_reports_command_ends_and_runs_the_users_files`, `zsh_follows_a_zdotdir_that_the_users_zshenv_sets`, `zsh_starts_interactive_with_the_wrapper_folder` and `install_writes_the_zsh_wrappers` as ok
- [ ] `cd src-tauri && cargo test -p terminal-grid pty::shell::live -- --nocapture 2>&1 | grep -c skipping` prints `0` on a machine that has zsh (`which zsh` prints a path). On a machine without zsh it prints `2` and the card is still fine
- [ ] `cd src-tauri && cargo test --workspace` passes
- [ ] `cd src-tauri && cargo fmt --check` exits 0
- [ ] `cd src-tauri && cargo check -p terminal-grid --no-default-features` exits 0
- [ ] `cd src-tauri && cargo clippy -p terminal-grid --all-targets 2>&1 | grep -c warning` prints the same number as the baseline in the Log, or a lower one
- [ ] `git status --porcelain | grep -v 'plans/002-trigger-internal/tasks/'` lists only paths under `files:`

## Out of scope

- bash, PowerShell and the `$HISTCMD` change in `shell-integration.sh`. They are T11, T13 and T12.
- Changing `tauri.conf.json`. The wrapper files are compiled in with `include_str!`; `resources/*` only bundles files directly under `resources/`, not the `zsh/` folder, and that is fine.
- Fixing the stale first prompt report in `shell-integration.sh` (T12).
- Editing docs, `plans/_context/repo-map.md` or the frontend.
- Fixing existing clippy warnings or renaming anything.

## If stuck

- If `cargo test` says `couldn't read ../../resources/zsh/.zshenv`, a wrapper file from step 2 is missing or misnamed (the names start with a dot). Check `ls -a src-tauri/resources/zsh`.
- If a live zsh test fails with `no OSC 133;D;1`, print the output it shows. If it contains `zsh-newuser-install` or a menu, the wrapper folder was not picked up: check that `integration_env` sets `ZDOTDIR` and that step 4 was applied.
- If the `ZDOTDIR leaked` assertion fails, copy the output into the Log and set `status: blocked`. Do not edit the assertion.
- If `cargo fmt` changes code given above, keep the formatted result; that is expected.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: clippy baseline 7 warnings; built, acceptance passed (clippy 7)
- 2026-10-12 checker: pass
