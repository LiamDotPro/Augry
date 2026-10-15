//! Helpers for the tests of `git.rs`. Tests that need git skip themselves with
//! a message when it is not installed, so `cargo test` passes on any machine.

use std::path::{Path, PathBuf};
use std::time::Duration;

use super::{run, GitOutput, TIMEOUT};

/// Commands in tests (clone, fetch, merge) get longer than the 3 s default so
/// a busy machine does not make them flaky.
const TEST_TIMEOUT: Duration = Duration::from_secs(30);

/// True when `git --version` runs. A test that needs git starts with
/// `if !git_available() { return; }`; the message goes to stderr.
pub fn git_available() -> bool {
    let ok = run(&std::env::temp_dir(), &["--version"], TIMEOUT)
        .map(|o| o.ok)
        .unwrap_or(false);
    if !ok {
        eprintln!("skipped: git is not installed");
    }
    ok
}

/// A path for a scratch directory inside the system temp dir. Not created.
pub fn scratch_path(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!("trigger-git-{label}-{}", uuid::Uuid::new_v4()))
}

/// A throwaway repository on branch `main` with a committer set, removed when
/// dropped. Tests build the situation they need with `git`, `write` and
/// `commit`.
pub struct TempRepo {
    pub path: PathBuf,
}

impl TempRepo {
    pub fn new() -> TempRepo {
        let path = scratch_path("repo");
        std::fs::create_dir_all(&path).unwrap();
        let repo = TempRepo { path };
        repo.git(&["init", "-q", "-b", "main"]);
        repo.configure();
        repo
    }

    fn configure(&self) {
        for (key, value) in [
            ("user.name", "Test"),
            ("user.email", "test@example.com"),
            ("commit.gpgsign", "false"),
            ("core.autocrlf", "false"),
        ] {
            self.git(&["config", key, value]);
        }
    }

    /// Runs git here and returns the output even when it exits non zero.
    pub fn try_git(&self, args: &[&str]) -> GitOutput {
        run(&self.path, args, TEST_TIMEOUT).unwrap()
    }

    /// Runs git here, panics if it fails, returns stdout without the final
    /// line break.
    pub fn git(&self, args: &[&str]) -> String {
        let out = self.try_git(args);
        assert!(out.ok, "git {args:?} failed: {}", out.stderr);
        out.stdout.trim_end().to_string()
    }

    /// Writes `content` to `name` under the work tree, making folders.
    pub fn write(&self, name: &str, content: &str) {
        let file = self.path.join(name);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, content).unwrap();
    }

    /// Writes `name` with the message as its content (so every commit really
    /// changes something), stages it and commits.
    pub fn commit(&self, name: &str, message: &str) {
        self.write(name, &format!("{message}\n"));
        self.git(&["add", name]);
        self.git(&["commit", "-q", "-m", message]);
    }

    /// Clones this repository into a new temporary one. The clone tracks
    /// `origin/main`. Keep `self` alive while the clone is used.
    pub fn clone_repo(&self) -> TempRepo {
        let path = scratch_path("clone");
        let source = self.path.to_str().unwrap();
        let target = path.to_str().unwrap();
        let out = run(
            &std::env::temp_dir(),
            &["clone", "-q", source, target],
            TEST_TIMEOUT,
        )
        .unwrap();
        assert!(out.ok, "clone failed: {}", out.stderr);
        let repo = TempRepo { path };
        repo.configure();
        repo
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}
