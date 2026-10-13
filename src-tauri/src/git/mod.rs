//! Git labels and worktrees. See docs/technical-design.md section 7.

pub mod cli;
pub mod review;
pub mod worktree;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::error::Result;
use crate::pty::TerminalId;

/// How long a cached entry is served without touching git.
pub const CACHE_TTL: Duration = Duration::from_secs(5);
/// A focus change refreshes anything older than this.
pub const FOCUS_MAX_AGE: Duration = Duration::from_secs(2);
/// Repositories above this many tracked files skip `git status`.
const LARGE_REPO_FILES: usize = 50_000;
const MAX_CONCURRENT_GIT: usize = 3;

pub const EVENT_GIT: &str = "terminal://git";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Worktree {
    pub path: String,
    pub head: Option<String>,
    pub branch: Option<String>,
    pub is_main: bool,
    pub is_current: bool,
    pub detached: bool,
    pub locked: bool,
    pub prunable: bool,
    pub open_in: Vec<TerminalId>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitInfo {
    pub in_repo: bool,
    pub repo_root: Option<String>,
    pub common_dir: Option<String>,
    pub repo_name: Option<String>,
    pub branch: Option<String>,
    pub head_short: Option<String>,
    pub user_name: Option<String>,
    /// True when the repo resolves `user.name` to something other than the
    /// global identity (a local setting or an `includeIf`).
    pub user_name_overridden: bool,
    pub user_email: Option<String>,
    pub remote_url: Option<String>,
    pub dirty: bool,
    pub ahead: u32,
    pub behind: u32,
    pub is_worktree: bool,
    pub worktrees: Vec<Worktree>,
}

impl GitInfo {
    fn not_a_repo() -> Self {
        Self {
            in_repo: false,
            repo_root: None,
            common_dir: None,
            repo_name: None,
            branch: None,
            head_short: None,
            user_name: None,
            user_name_overridden: false,
            user_email: None,
            remote_url: None,
            dirty: false,
            ahead: 0,
            behind: 0,
            is_worktree: false,
            worktrees: Vec::new(),
        }
    }
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GitEvent {
    pub id: TerminalId,
    pub info: GitInfo,
}

struct Cached {
    info: GitInfo,
    at: Instant,
}

/// Caps how many `git` processes exist at once (design section 13).
struct Semaphore {
    slots: Mutex<usize>,
    available: Condvar,
}

impl Semaphore {
    fn new(count: usize) -> Self {
        Self {
            slots: Mutex::new(count),
            available: Condvar::new(),
        }
    }

    fn acquire(&self) -> SemaphoreGuard<'_> {
        let mut slots = self.slots.lock().expect("git semaphore");
        while *slots == 0 {
            slots = self.available.wait(slots).expect("git semaphore");
        }
        *slots -= 1;
        SemaphoreGuard { semaphore: self }
    }
}

struct SemaphoreGuard<'a> {
    semaphore: &'a Semaphore,
}

impl Drop for SemaphoreGuard<'_> {
    fn drop(&mut self) {
        *self.semaphore.slots.lock().expect("git semaphore") += 1;
        self.semaphore.available.notify_one();
    }
}

pub struct GitService {
    cache: Mutex<HashMap<String, Cached>>,
    /// Repo roots known to be too large for `git status`.
    large: Mutex<HashSet<String>>,
    permits: Semaphore,
}

impl Default for GitService {
    fn default() -> Self {
        Self::new()
    }
}

impl GitService {
    pub fn new() -> Self {
        Self {
            cache: Mutex::new(HashMap::new()),
            large: Mutex::new(HashSet::new()),
            permits: Semaphore::new(MAX_CONCURRENT_GIT),
        }
    }

    /// Repository facts for `cwd`, refreshed when the cached copy is older than
    /// `max_age`. `open_dirs` fills in which other panes hold each worktree.
    pub fn info(
        &self,
        cwd: &Path,
        max_age: Duration,
        open_dirs: &[(TerminalId, PathBuf)],
    ) -> GitInfo {
        let key = key_for(cwd);

        if let Some(cached) = self.cache.lock().expect("git cache").get(&key) {
            if cached.at.elapsed() < max_age {
                return with_open_in(cached.info.clone(), open_dirs);
            }
        }

        let info = {
            let _permit = self.permits.acquire();
            self.collect(cwd)
        };

        self.cache.lock().expect("git cache").insert(
            key,
            Cached {
                info: info.clone(),
                at: Instant::now(),
            },
        );
        with_open_in(info, open_dirs)
    }

    pub fn invalidate_all(&self) {
        self.cache.lock().expect("git cache").clear();
    }

    fn collect(&self, cwd: &Path) -> GitInfo {
        // One invocation covers the four cheap lookups.
        let Some(base) = cli::output(
            cwd,
            &[
                "rev-parse",
                "--show-toplevel",
                "--git-common-dir",
                "--abbrev-ref",
                "HEAD",
                "--short",
                "HEAD",
            ],
        ) else {
            return GitInfo::not_a_repo();
        };

        let mut lines = base.lines();
        let Some(repo_root) = lines.next().map(worktree::normalise) else {
            return GitInfo::not_a_repo();
        };
        let common_dir = lines
            .next()
            .map(|dir| absolutise(&repo_root, &worktree::normalise(dir)));
        let branch = lines.next().map(str::to_string).filter(|b| b != "HEAD");
        let head_short = lines.next().map(str::to_string);

        let main_root = common_dir
            .as_deref()
            .and_then(parent_of_git_dir)
            .unwrap_or_else(|| repo_root.clone());

        let dirty = if self.is_large(&main_root, cwd) {
            false
        } else {
            cli::output(cwd, &["status", "--porcelain", "--untracked-files=no"])
                .is_some_and(|status| !status.trim().is_empty())
        };

        let (ahead, behind) = cli::value(
            cwd,
            &["rev-list", "--left-right", "--count", "@{upstream}...HEAD"],
        )
        .and_then(|counts| {
            let mut parts = counts.split_whitespace();
            let behind = parts.next()?.parse().ok()?;
            let ahead = parts.next()?.parse().ok()?;
            Some((ahead, behind))
        })
        .unwrap_or((0, 0));

        let worktrees = cli::output(cwd, &["worktree", "list", "--porcelain"])
            .map(|porcelain| worktree::parse(&porcelain))
            .unwrap_or_default();

        let current = key_for(cwd);
        let worktrees: Vec<Worktree> = worktrees
            .into_iter()
            .enumerate()
            .map(|(index, parsed)| {
                let path = worktree::normalise(&parsed.path.to_string_lossy());
                Worktree {
                    is_current: is_inside(&current, &path),
                    is_main: index == 0,
                    head: parsed.head,
                    branch: parsed.branch,
                    detached: parsed.detached,
                    locked: parsed.locked,
                    prunable: parsed.prunable,
                    open_in: Vec::new(),
                    path,
                }
            })
            .collect();

        let user_name = cli::value(cwd, &["config", "--get", "user.name"]);
        let user_name_overridden = user_name.is_some()
            && user_name != cli::value(cwd, &["config", "--global", "--get", "user.name"]);

        GitInfo {
            in_repo: true,
            repo_name: last_segment(&main_root),
            is_worktree: !same_path(&main_root, &repo_root),
            user_name,
            user_name_overridden,
            user_email: cli::value(cwd, &["config", "--get", "user.email"]),
            remote_url: cli::value(cwd, &["remote", "get-url", "origin"]),
            repo_root: Some(repo_root),
            common_dir,
            branch,
            head_short,
            dirty,
            ahead,
            behind,
            worktrees,
        }
    }

    /// `git status` is skipped on very large repositories; the answer is cached
    /// per repo so the file count is only ever taken once.
    fn is_large(&self, main_root: &str, cwd: &Path) -> bool {
        let key = key_for(Path::new(main_root));
        if self.large.lock().expect("git large").contains(&key) {
            return true;
        }
        let Some(listing) = cli::output(cwd, &["ls-files"]) else {
            return false;
        };
        if listing.lines().count() <= LARGE_REPO_FILES {
            return false;
        }
        self.large.lock().expect("git large").insert(key);
        true
    }
}

/// Marks, for each worktree, which other panes have a cwd inside it.
fn with_open_in(mut info: GitInfo, open_dirs: &[(TerminalId, PathBuf)]) -> GitInfo {
    for worktree in &mut info.worktrees {
        worktree.open_in = open_dirs
            .iter()
            .filter(|(_, dir)| is_inside(&key_for(dir), &worktree.path))
            .map(|(id, _)| id.clone())
            .collect();
    }
    info
}

// Path helpers ------------------------------------------------------------

/// Comparison form: forward slashes, no trailing slash, case folded on Windows
/// because NTFS paths are case insensitive.
fn key_for(path: &Path) -> String {
    let text = worktree::normalise(&path.to_string_lossy());
    let text = text.trim_end_matches('/').to_string();
    if cfg!(windows) {
        text.to_lowercase()
    } else {
        text
    }
}

fn same_path(a: &str, b: &str) -> bool {
    key_for(Path::new(a)) == key_for(Path::new(b))
}

/// True when `child` is the same as, or below, `parent`.
fn is_inside(child_key: &str, parent: &str) -> bool {
    let parent_key = key_for(Path::new(parent));
    child_key == parent_key || child_key.starts_with(&format!("{parent_key}/"))
}

/// `git rev-parse --git-common-dir` answers `.git` for the main worktree.
fn absolutise(repo_root: &str, dir: &str) -> String {
    if dir == ".git" {
        format!("{repo_root}/.git")
    } else if Path::new(dir).is_absolute() {
        dir.to_string()
    } else {
        format!("{repo_root}/{dir}")
    }
}

/// The main repository is the parent of the common `.git` directory.
fn parent_of_git_dir(common_dir: &str) -> Option<String> {
    let trimmed = common_dir.trim_end_matches('/');
    let parent = trimmed.strip_suffix("/.git").unwrap_or_else(|| {
        Path::new(trimmed)
            .parent()
            .and_then(|p| p.to_str())
            .unwrap_or(trimmed)
    });
    Some(worktree::normalise(parent))
}

fn last_segment(path: &str) -> Option<String> {
    path.trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|segment| !segment.is_empty())
        .map(str::to_string)
}

// Commands ----------------------------------------------------------------

/// Both git commands hand the subprocess work to a blocking thread: a repo on a
/// cold cache costs several `git` invocations and must not tie up an async
/// runtime worker while nine panes ask at once.
#[tauri::command]
pub async fn get_git_info(app: AppHandle, id: TerminalId) -> Result<GitInfo> {
    crate::blocking(move || {
        let state = app.state::<crate::AppState>();
        let Some(cwd) = state.pty.cwd_of(&id) else {
            return GitInfo::not_a_repo();
        };
        let open_dirs = state.pty.open_dirs();
        state.git.info(&cwd, CACHE_TTL, &open_dirs)
    })
    .await
}

#[tauri::command]
pub async fn refresh_git_info(app: AppHandle, id: TerminalId) -> Result<()> {
    crate::blocking(move || {
        let state = app.state::<crate::AppState>();
        refresh_repo_of(&app, &state, &id, Duration::ZERO);
    })
    .await
}

/// Recomputes `id` and fans the result out to every pane in the same repository,
/// which is what makes `git worktree add` in one pane show up in the others.
pub fn refresh_repo_of(
    app: &AppHandle,
    state: &crate::AppState,
    id: &TerminalId,
    max_age: Duration,
) {
    let Some(cwd) = state.pty.cwd_of(id) else {
        return;
    };
    let open_dirs = state.pty.open_dirs();
    let info = state.git.info(&cwd, max_age, &open_dirs);
    let common_dir = info.common_dir.clone();
    emit_git(app, id.clone(), info);

    let Some(common_dir) = common_dir else {
        return;
    };
    for (other_id, other_cwd) in open_dirs.iter() {
        if other_id == id {
            continue;
        }
        let other = state.git.info(other_cwd, max_age, &open_dirs);
        if other.common_dir.as_deref() == Some(common_dir.as_str()) {
            emit_git(app, other_id.clone(), other);
        }
    }
}

pub fn emit_git(app: &AppHandle, id: TerminalId, info: GitInfo) {
    let _ = app.emit_to("main", EVENT_GIT, GitEvent { id, info });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(windows)]
    fn paths_compare_case_insensitively_on_windows() {
        let a = key_for(Path::new(r"C:\Dev\App"));
        let b = key_for(Path::new("c:/dev/app/"));
        assert_eq!(a, b);
    }

    #[test]
    #[cfg(not(windows))]
    fn paths_compare_case_sensitively_elsewhere() {
        assert_eq!(
            key_for(Path::new("/dev/app/")),
            key_for(Path::new("/dev/app"))
        );
        assert_ne!(
            key_for(Path::new("/Dev/App")),
            key_for(Path::new("/dev/app"))
        );
    }

    #[test]
    fn is_inside_matches_the_directory_itself_and_its_children() {
        let child = key_for(Path::new("C:/dev/app/src"));
        assert!(is_inside(&child, "C:/dev/app"));
        assert!(is_inside(&key_for(Path::new("C:/dev/app")), "C:/dev/app"));
    }

    #[test]
    fn is_inside_does_not_match_a_sibling_with_a_shared_prefix() {
        let sibling = key_for(Path::new("C:/dev/app-feature"));
        assert!(!is_inside(&sibling, "C:/dev/app"));
    }

    #[test]
    fn common_dir_resolves_to_the_main_repository() {
        assert_eq!(
            parent_of_git_dir("C:/dev/app/.git").as_deref(),
            Some("C:/dev/app")
        );
        assert_eq!(
            parent_of_git_dir("C:/dev/app/.git/worktrees/feature")
                .as_deref()
                .map(|p| p.contains("worktrees")),
            Some(true)
        );
    }

    #[test]
    fn a_relative_common_dir_is_resolved_against_the_repo_root() {
        assert_eq!(absolutise("C:/dev/app", ".git"), "C:/dev/app/.git");
        assert_eq!(
            absolutise("/dev/app", "../shared.git"),
            "/dev/app/../shared.git"
        );
    }

    #[test]
    fn an_absolute_common_dir_is_kept() {
        let dir = if cfg!(windows) {
            "C:/dev/app/.git"
        } else {
            "/dev/app/.git"
        };
        assert_eq!(absolutise("/elsewhere", dir), dir);
    }

    #[test]
    fn repo_name_is_the_last_path_segment() {
        assert_eq!(
            last_segment("C:/dev/augry").as_deref(),
            Some("augry")
        );
        assert_eq!(
            last_segment("C:/dev/augry/").as_deref(),
            Some("augry")
        );
    }

    #[test]
    fn open_in_lists_every_pane_inside_a_worktree() {
        let info = GitInfo {
            worktrees: vec![Worktree {
                path: "C:/dev/app".to_string(),
                head: None,
                branch: Some("main".to_string()),
                is_main: true,
                is_current: true,
                detached: false,
                locked: false,
                prunable: false,
                open_in: Vec::new(),
            }],
            ..GitInfo::not_a_repo()
        };
        let dirs = vec![
            ("a".to_string(), PathBuf::from("C:/dev/app/src")),
            ("b".to_string(), PathBuf::from("C:/dev/other")),
        ];
        let filled = with_open_in(info, &dirs);
        assert_eq!(filled.worktrees[0].open_in, vec!["a".to_string()]);
    }
}
