//! Worktrees: the parser for `git worktree list --porcelain` (see
//! docs/technical-design.md section 7.3), and creating and removing them from
//! the branch menu (section 7.5).

use std::path::{Path, PathBuf};
use std::time::Duration;

use tauri::{AppHandle, Manager};

use super::cli;
use crate::error::{AppError, Result};
use crate::pty::TerminalId;
use crate::triggers::feeds;
use crate::triggers::internal::InternalSignal;

/// Checking out a large repository into a new folder takes a while.
const TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedWorktree {
    pub path: PathBuf,
    pub head: Option<String>,
    /// `refs/heads/x` reduced to `x`.
    pub branch: Option<String>,
    pub bare: bool,
    pub detached: bool,
    pub locked: bool,
    pub prunable: bool,
}

/// Blocks are separated by blank lines and always start with `worktree <path>`.
/// The first block is the main worktree. Unknown attribute lines are ignored so
/// a newer git can add fields without breaking this.
pub fn parse(porcelain: &str) -> Vec<ParsedWorktree> {
    let mut worktrees = Vec::new();
    let mut current: Option<ParsedWorktree> = None;

    for line in porcelain.lines() {
        let line = line.trim_end_matches('\r');
        if line.is_empty() {
            worktrees.extend(current.take());
            continue;
        }

        let (key, rest) = match line.split_once(' ') {
            Some((key, rest)) => (key, rest),
            None => (line, ""),
        };

        match key {
            "worktree" => {
                worktrees.extend(current.take());
                current = Some(ParsedWorktree {
                    path: PathBuf::from(normalise(rest)),
                    head: None,
                    branch: None,
                    bare: false,
                    detached: false,
                    locked: false,
                    prunable: false,
                });
            }
            "HEAD" => {
                if let Some(worktree) = current.as_mut() {
                    worktree.head = Some(rest.to_string());
                }
            }
            "branch" => {
                if let Some(worktree) = current.as_mut() {
                    worktree.branch = Some(rest.trim_start_matches("refs/heads/").to_string());
                }
            }
            "bare" => {
                if let Some(worktree) = current.as_mut() {
                    worktree.bare = true;
                }
            }
            "detached" => {
                if let Some(worktree) = current.as_mut() {
                    worktree.detached = true;
                }
            }
            "locked" => {
                if let Some(worktree) = current.as_mut() {
                    worktree.locked = true;
                }
            }
            "prunable" => {
                if let Some(worktree) = current.as_mut() {
                    worktree.prunable = true;
                }
            }
            _ => {}
        }
    }

    worktrees.extend(current);
    worktrees
}

/// Git prints forward slashes even on Windows; normalise anything else we meet
/// so path comparisons elsewhere have one shape to deal with.
pub fn normalise(path: &str) -> String {
    path.trim().replace('\\', "/")
}

// Creating and removing ------------------------------------------------

/// Adds a worktree for `branch` next to the main checkout (`../app-feature-x`)
/// and returns its path. An existing branch, local or on one remote, is checked
/// out; any other name becomes a new branch from the pane's HEAD.
pub fn create(cwd: &Path, branch: &str) -> Result<String> {
    let branch = branch.trim();
    let valid = !branch.is_empty() && cli::run(cwd, &["check-ref-format", "--branch", branch])?.ok;
    if !valid {
        return Err(AppError::InvalidName {
            message: format!("\"{branch}\" is not a valid branch name"),
        });
    }

    let main_root = main_root(cwd)?;
    let target = free_path(&sibling_path(&main_root, branch)?);
    let target_text = normalise(&target.to_string_lossy());

    let local = cli::value(
        cwd,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("refs/heads/{branch}"),
        ],
    )
    .is_some();
    // git checks out (and tracks) a branch that exists on exactly one remote.
    let remotes = cli::output(
        cwd,
        &[
            "for-each-ref",
            "--format=%(refname)",
            &format!("refs/remotes/*/{branch}"),
        ],
    )
    .map(|refs| refs.lines().filter(|line| !line.is_empty()).count())
    .unwrap_or(0);

    if local || remotes == 1 {
        git(cwd, &["worktree", "add", &target_text, branch])?;
    } else {
        git(cwd, &["worktree", "add", "-b", branch, &target_text])?;
    }
    Ok(target_text)
}

/// Removes a linked worktree's folder; the branch stays. A worktree whose
/// folder is already gone is pruned instead.
pub fn remove(cwd: &Path, path: &str) -> Result<()> {
    let listed = cli::output(cwd, &["worktree", "list", "--porcelain"])
        .map(|porcelain| parse(&porcelain))
        .unwrap_or_default();
    let key = super::key_for(Path::new(path));
    let Some((index, worktree)) = listed
        .iter()
        .enumerate()
        .find(|(_, worktree)| super::key_for(&worktree.path) == key)
    else {
        return Err(AppError::NotFound {
            message: format!("{path} is not a worktree of this repository"),
        });
    };

    let refuse = |message: &str| {
        Err(AppError::InvalidName {
            message: message.to_string(),
        })
    };
    if index == 0 {
        return refuse("The main checkout cannot be removed.");
    }
    if worktree.locked {
        return refuse("This worktree is locked. Unlock it with git worktree unlock first.");
    }
    if super::is_inside(&super::key_for(cwd), path) {
        return refuse("This pane is inside that worktree. Move it out first.");
    }

    if worktree.prunable {
        return git(cwd, &["worktree", "prune"]);
    }
    git(cwd, &["worktree", "remove", path]).map_err(|error| match error {
        AppError::Io { message } if message.contains("modified or untracked files") => {
            AppError::InvalidName {
                message: "It has uncommitted changes. Commit or stash them first, then remove it."
                    .to_string(),
            }
        }
        other => other,
    })
}

fn git(cwd: &Path, args: &[&str]) -> Result<()> {
    let output = cli::run_raw(cwd, args, TIMEOUT)?;
    if output.ok {
        return Ok(());
    }
    let message = output.stderr.trim();
    Err(AppError::Io {
        message: message
            .strip_prefix("fatal: ")
            .unwrap_or(message)
            .to_string(),
    })
}

/// The main checkout, wherever in the repository `cwd` sits.
fn main_root(cwd: &Path) -> Result<String> {
    let not_a_repo = || AppError::NotFound {
        message: "this pane is not inside a git repository".to_string(),
    };
    let base = cli::output(cwd, &["rev-parse", "--show-toplevel", "--git-common-dir"])
        .ok_or_else(not_a_repo)?;
    let mut lines = base.lines();
    let repo_root = lines.next().map(normalise).ok_or_else(not_a_repo)?;
    Ok(lines
        .next()
        .map(|dir| super::absolutise(&repo_root, &normalise(dir)))
        .as_deref()
        .and_then(super::parent_of_git_dir)
        .unwrap_or(repo_root))
}

/// `C:/dev/app` and `feature/x` give `C:/dev/app-feature-x`.
fn sibling_path(main_root: &str, branch: &str) -> Result<PathBuf> {
    let main = Path::new(main_root);
    let (Some(parent), Some(repo)) = (main.parent(), main.file_name()) else {
        return Err(AppError::InvalidName {
            message: format!("no folder next to {main_root} to put the worktree in"),
        });
    };
    let slug: String = branch
        .chars()
        .map(|c| {
            if c == '/' || c == '\\' || c.is_whitespace() {
                '-'
            } else {
                c
            }
        })
        .collect();
    Ok(parent.join(format!("{}-{slug}", repo.to_string_lossy())))
}

/// `path`, or `path-2`, `path-3`... when something is already there.
fn free_path(path: &Path) -> PathBuf {
    if !path.exists() {
        return path.to_path_buf();
    }
    (2..)
        .map(|n| PathBuf::from(format!("{}-{n}", path.to_string_lossy())))
        .find(|candidate| !candidate.exists())
        .expect("some suffix is free")
}

fn cwd_of(app: &AppHandle, id: &TerminalId) -> Result<PathBuf> {
    app.state::<crate::AppState>()
        .pty
        .cwd_of(id)
        .ok_or_else(|| AppError::NotFound {
            message: format!("terminal {id}"),
        })
}

/// Every pane of the repository picks the change up straight away.
fn refresh(app: &AppHandle, id: &TerminalId) {
    let state = app.state::<crate::AppState>();
    super::refresh_repo_of(app, &state, id, Duration::ZERO);
}

/// `branch` is what the user typed; `create` trims it the same way. An empty
/// name cannot get this far, but it would be left out rather than sent.
fn created_signal(repo: String, path: &str, branch: &str) -> InternalSignal {
    let branch = branch.trim();
    InternalSignal::WorktreeCreated {
        repo,
        path: normalise(path),
        branch: (!branch.is_empty()).then(|| branch.to_string()),
    }
}

fn removed_signal(repo: String, path: &str) -> InternalSignal {
    InternalSignal::WorktreeRemoved {
        repo,
        path: normalise(path),
    }
}

#[tauri::command]
pub async fn worktree_create(app: AppHandle, id: TerminalId, branch: String) -> Result<String> {
    crate::blocking(move || {
        let cwd = cwd_of(&app, &id)?;
        let path = create(&cwd, &branch)?;
        refresh(&app, &id);
        let state = app.state::<crate::AppState>();
        if let Some(repo) = feeds::repo_root_of(&state, &cwd) {
            feeds::publish(&app, created_signal(repo, &path, &branch));
        }
        Ok(path)
    })
    .await?
}

#[tauri::command]
pub async fn worktree_remove(app: AppHandle, id: TerminalId, path: String) -> Result<()> {
    crate::blocking(move || {
        let cwd = cwd_of(&app, &id)?;
        remove(&cwd, &path)?;
        refresh(&app, &id);
        let state = app.state::<crate::AppState>();
        if let Some(repo) = feeds::repo_root_of(&state, &cwd) {
            feeds::publish(&app, removed_signal(repo, &path));
        }
        Ok(())
    })
    .await?
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "worktree C:/dev/app\n\
                          HEAD 3f2a91c0000000000000000000000000000000aa\n\
                          branch refs/heads/main\n\
                          \n\
                          worktree C:/dev/app-feature\n\
                          HEAD 91bc000000000000000000000000000000000bb\n\
                          branch refs/heads/feature/x\n\
                          locked\n\
                          \n\
                          worktree C:/dev/app-old\n\
                          HEAD 77ab000000000000000000000000000000000cc\n\
                          detached\n\
                          prunable gitdir file points to non-existent location\n";

    #[test]
    fn a_created_worktree_names_the_trimmed_branch_and_a_forward_slash_path() {
        let signal = created_signal("C:/dev/app".into(), "C:\\dev\\app-x", " x ");
        assert_eq!(
            signal,
            InternalSignal::WorktreeCreated {
                repo: "C:/dev/app".into(),
                path: "C:/dev/app-x".into(),
                branch: Some("x".into()),
            }
        );
    }

    #[test]
    fn a_blank_branch_is_left_out_of_the_signal() {
        let InternalSignal::WorktreeCreated { branch, .. } =
            created_signal("/a".into(), "/b", "  ")
        else {
            panic!("expected WorktreeCreated");
        };
        assert_eq!(branch, None);
    }

    #[test]
    fn a_removed_worktree_names_its_forward_slash_path() {
        assert_eq!(
            removed_signal("/a".into(), "C:\\dev\\app-x"),
            InternalSignal::WorktreeRemoved {
                repo: "/a".into(),
                path: "C:/dev/app-x".into(),
            }
        );
    }

    #[test]
    fn reads_every_block() {
        let worktrees = parse(SAMPLE);
        assert_eq!(worktrees.len(), 3);
        assert_eq!(worktrees[0].branch.as_deref(), Some("main"));
        assert_eq!(worktrees[1].branch.as_deref(), Some("feature/x"));
        assert!(worktrees[1].locked);
        assert!(worktrees[2].detached);
        assert!(worktrees[2].prunable);
        assert_eq!(worktrees[2].branch, None);
    }

    #[test]
    fn the_first_block_is_the_main_worktree() {
        assert_eq!(parse(SAMPLE)[0].path, PathBuf::from("C:/dev/app"));
    }

    #[test]
    fn handles_paths_with_spaces() {
        let worktrees = parse("worktree C:/dev/my app\nHEAD abc\nbranch refs/heads/main\n");
        assert_eq!(worktrees[0].path, PathBuf::from("C:/dev/my app"));
    }

    #[test]
    fn handles_a_bare_repository() {
        let worktrees = parse("worktree C:/dev/bare.git\nbare\n");
        assert_eq!(worktrees.len(), 1);
        assert!(worktrees[0].bare);
        assert_eq!(worktrees[0].head, None);
    }

    #[test]
    fn tolerates_missing_trailing_newline_and_crlf() {
        let worktrees = parse("worktree C:/a\r\nHEAD abc\r\nbranch refs/heads/main");
        assert_eq!(worktrees.len(), 1);
        assert_eq!(worktrees[0].branch.as_deref(), Some("main"));
        assert_eq!(worktrees[0].head.as_deref(), Some("abc"));
    }

    #[test]
    fn ignores_unknown_attributes() {
        let worktrees = parse("worktree C:/a\nHEAD abc\nsomething-new value\n");
        assert_eq!(worktrees.len(), 1);
        assert_eq!(worktrees[0].head.as_deref(), Some("abc"));
    }

    #[test]
    fn empty_input_yields_nothing() {
        assert!(parse("").is_empty());
        assert!(parse("\n\n").is_empty());
    }

    #[test]
    fn normalises_backslashes() {
        assert_eq!(normalise(r"C:\dev\app"), "C:/dev/app");
    }

    fn git_in(dir: &Path, args: &[&str]) {
        let status = std::process::Command::new("git")
            .args(args)
            .current_dir(dir)
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "test@example.com")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "test@example.com")
            .status()
            .expect("git runs");
        assert!(status.success(), "git {args:?}");
    }

    #[test]
    fn sibling_paths_flatten_the_branch() {
        assert_eq!(
            sibling_path("C:/dev/app", "feature/x").unwrap(),
            PathBuf::from("C:/dev/app-feature-x")
        );
    }

    #[test]
    fn create_and_remove_round_trip() {
        let base = std::env::temp_dir().join(format!("tg-worktree-{}", uuid::Uuid::new_v4()));
        let main = base.join("app");
        std::fs::create_dir_all(&main).unwrap();
        let main = main.canonicalize().unwrap();
        git_in(&main, &["init", "-q", "-b", "main"]);
        std::fs::write(main.join("a.txt"), "one\n").unwrap();
        git_in(&main, &["add", "a.txt"]);
        git_in(
            &main,
            &["-c", "commit.gpgsign=false", "commit", "-q", "-m", "init"],
        );
        git_in(&main, &["branch", "existing"]);

        // A new name becomes a new branch next to the main checkout.
        let fresh = create(&main, "feature/x").unwrap();
        assert!(fresh.ends_with("/app-feature-x"));
        assert!(Path::new(&fresh).join("a.txt").exists());

        // An existing branch is checked out, from inside another worktree too.
        let existing = create(Path::new(&fresh), "existing").unwrap();
        assert!(existing.ends_with("/app-existing"));

        // A folder already in the way gets a suffix.
        std::fs::create_dir_all(base.join("app-taken")).unwrap();
        assert!(create(&main, "taken").unwrap().ends_with("/app-taken-2"));

        assert!(create(&main, "bad..name").is_err());
        assert!(create(&main, "existing").is_err(), "already checked out");

        // Uncommitted work keeps a worktree; the main checkout is never removed.
        std::fs::write(Path::new(&existing).join("a.txt"), "two\n").unwrap();
        assert!(remove(&main, &existing).is_err());
        assert!(remove(Path::new(&fresh), &normalise(&main.to_string_lossy())).is_err());
        assert!(
            remove(Path::new(&fresh), &fresh).is_err(),
            "the pane is inside it"
        );

        remove(&main, &fresh).unwrap();
        assert!(!Path::new(&fresh).exists());
        let listed = parse(&cli::output(&main, &["worktree", "list", "--porcelain"]).unwrap());
        assert_eq!(listed.len(), 3);

        let _ = std::fs::remove_dir_all(&base);
    }
}
