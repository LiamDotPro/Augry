//! Shell discovery and the launch arguments that pull in the integration
//! script. See docs/technical-design.md sections 4.2 and 4.5.

use std::fs;
use std::path::Path;
#[cfg(windows)]
use std::sync::OnceLock;

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ShellKind {
    Pwsh,
    WindowsPowerShell,
    Cmd,
    Bash,
    Zsh,
    Other,
}

impl ShellKind {
    /// Classifies a shell by its executable name, ignoring path and extension.
    pub fn from_program(program: &str) -> Self {
        let stem = Path::new(program)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(program)
            .to_ascii_lowercase();
        match stem.as_str() {
            "pwsh" | "pwsh-preview" => ShellKind::Pwsh,
            "powershell" => ShellKind::WindowsPowerShell,
            "cmd" => ShellKind::Cmd,
            "bash" | "sh" => ShellKind::Bash,
            "zsh" => ShellKind::Zsh,
            _ => ShellKind::Other,
        }
    }

    fn is_powershell(self) -> bool {
        matches!(self, ShellKind::Pwsh | ShellKind::WindowsPowerShell)
    }
}

pub struct Shell {
    pub program: String,
    pub kind: ShellKind,
    pub args: Vec<String>,
    /// Variables the integration needs, set on the child on top of the app's own.
    pub env: Vec<(String, String)>,
}

/// Resolves the shell to launch: the configured one, else the best available.
///
/// Windows order is pwsh, then Windows PowerShell, then `%COMSPEC%`; elsewhere
/// `$SHELL`, then `/bin/bash`.
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
    let program = configured
        .map(str::to_string)
        .filter(|p| !p.trim().is_empty())
        .unwrap_or_else(default_program);
    let kind = ShellKind::from_program(&program);

    // `sh` is classed as bash for the frontend's sake, but it may be dash,
    // which has no --rcfile, so it starts bare.
    let mut args = if is_plain_sh(&program) {
        Vec::new()
    } else {
        integration_args(kind, script_dir)
    };
    args.extend(extra_args.iter().cloned());

    Shell {
        program,
        kind,
        args,
        env: integration_env(kind, script_dir, user_zdotdir, home),
    }
}

#[cfg(windows)]
fn default_program() -> String {
    if which("pwsh.exe") {
        return "pwsh.exe".to_string();
    }
    if which("powershell.exe") {
        return "powershell.exe".to_string();
    }
    std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string())
}

#[cfg(not(windows))]
fn default_program() -> String {
    std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string())
}

fn is_plain_sh(program: &str) -> bool {
    Path::new(program)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .is_some_and(|stem| stem.eq_ignore_ascii_case("sh"))
}

fn integration_args(kind: ShellKind, script_dir: &Path) -> Vec<String> {
    if kind.is_powershell() {
        let script = script_dir.join("shell-integration.ps1");
        return vec![
            "-NoLogo".to_string(),
            "-NoExit".to_string(),
            "-Command".to_string(),
            format!(". '{}'", script.display().to_string().replace('\'', "''")),
        ];
    }
    if kind == ShellKind::Zsh {
        // The wrapper dotfiles in `ZDOTDIR` (see `integration_env`) load the
        // integration, so zsh just starts interactive. The old `-c '. script;
        // exec zsh -i'` lost the script's functions in the `exec`.
        return vec!["-i".to_string()];
    }
    if kind == ShellKind::Bash {
        // A wrapper rc file runs the user's own ~/.bashrc, then the integration.
        // `-i` is needed because --rcfile is only read by interactive shells.
        return vec![
            "--rcfile".to_string(),
            script_dir.join("bashrc").display().to_string(),
            "-i".to_string(),
        ];
    }
    // cmd.exe and anything unrecognised run bare; the process watcher still works.
    Vec::new()
}

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
const WRAPPERS: [(&str, &str); 5] = [
    ("zsh/.zshenv", include_str!("../../resources/zsh/.zshenv")),
    (
        "zsh/.zprofile",
        include_str!("../../resources/zsh/.zprofile"),
    ),
    ("zsh/.zshrc", include_str!("../../resources/zsh/.zshrc")),
    ("zsh/.zlogin", include_str!("../../resources/zsh/.zlogin")),
    ("bashrc", include_str!("../../resources/bashrc")),
];

/// Copies the bundled integration scripts into the app data dir on first run.
/// They are re-copied on every launch so an app update refreshes them.
///
/// Only the contents are copied. `fs::copy` on Windows also copies alternate
/// data streams, so a script installed from a downloaded installer kept its
/// Mark of the Web and RemoteSigned refused to run it. Removing the old copy
/// first matters too: rewriting a file in place keeps its streams.
pub fn install_scripts(resource_dir: &Path, target_dir: &Path) -> std::io::Result<()> {
    fs::create_dir_all(target_dir)?;
    for name in ["shell-integration.ps1", "shell-integration.sh"] {
        let source = resource_dir.join("resources").join(name);
        let source = if source.exists() {
            source
        } else {
            resource_dir.join(name)
        };
        if source.exists() {
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

#[cfg(windows)]
fn which(program: &str) -> bool {
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};

    static CACHE: OnceLock<std::sync::Mutex<std::collections::HashMap<String, bool>>> =
        OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some(found) = cache.lock().expect("which cache").get(program) {
        return *found;
    }

    let found = Command::new("where.exe")
        .arg(program)
        .creation_flags(crate::git::cli::CREATE_NO_WINDOW)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false);

    cache
        .lock()
        .expect("which cache")
        .insert(program.to_string(), found);
    found
}

#[cfg(all(test, unix))]
mod live;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_shells_by_executable_name() {
        assert_eq!(ShellKind::from_program("pwsh.exe"), ShellKind::Pwsh);
        // Backslashes only separate path segments on Windows.
        #[cfg(windows)]
        assert_eq!(
            ShellKind::from_program(r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.EXE"),
            ShellKind::WindowsPowerShell
        );
        assert_eq!(
            ShellKind::from_program("/usr/local/bin/pwsh"),
            ShellKind::Pwsh
        );
        assert_eq!(ShellKind::from_program("/bin/zsh"), ShellKind::Zsh);
        assert_eq!(ShellKind::from_program("nu"), ShellKind::Other);
    }

    #[test]
    fn powershell_gets_the_dot_sourced_script() {
        let args = integration_args(ShellKind::Pwsh, Path::new("C:/data"));
        assert_eq!(args[0], "-NoLogo");
        assert!(args.last().unwrap().contains("shell-integration.ps1"));
    }

    #[test]
    #[cfg(windows)]
    fn installed_scripts_drop_the_mark_of_the_web() {
        let root = std::env::temp_dir().join(format!("tg-install-{}", uuid::Uuid::new_v4()));
        let resources = root.join("resources");
        let target = root.join("data");
        fs::create_dir_all(&resources).unwrap();
        fs::create_dir_all(&target).unwrap();

        let mark = "[ZoneTransfer]\r\nZoneId=3\r\n";
        let source = resources.join("shell-integration.ps1");
        fs::write(&source, "# integration").unwrap();
        fs::write(format!("{}:Zone.Identifier", source.display()), mark).unwrap();
        // An older install whose copy already carries the mark.
        let installed = target.join("shell-integration.ps1");
        fs::write(&installed, "# old").unwrap();
        fs::write(format!("{}:Zone.Identifier", installed.display()), mark).unwrap();

        install_scripts(&root, &target).unwrap();

        assert_eq!(fs::read_to_string(&installed).unwrap(), "# integration");
        assert!(fs::metadata(format!("{}:Zone.Identifier", installed.display())).is_err());
        fs::remove_dir_all(&root).unwrap();
    }

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

    #[test]
    fn powershell_script_reports_the_native_exit_code() {
        let script = include_str!("../../resources/shell-integration.ps1");
        assert!(script.contains("$global:LASTEXITCODE"));
        // $? only means something if nothing runs before it is read.
        let body = script.split("function global:prompt {").nth(1).unwrap();
        let first = body
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty() && !line.starts_with('#'))
            .unwrap();
        assert_eq!(first, "$ok = $?");
    }

    #[test]
    fn cmd_launches_bare() {
        assert!(integration_args(ShellKind::Cmd, Path::new("C:/data")).is_empty());
    }

    #[test]
    fn configured_shell_wins_and_extra_args_follow() {
        let shell = resolve(
            Some("cmd.exe"),
            &["/K".to_string(), "echo hi".to_string()],
            Path::new("C:/data"),
        );
        assert_eq!(shell.program, "cmd.exe");
        assert_eq!(shell.kind, ShellKind::Cmd);
        assert_eq!(shell.args, vec!["/K".to_string(), "echo hi".to_string()]);
    }
}
