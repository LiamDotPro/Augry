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
