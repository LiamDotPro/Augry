//! Focus + review (design turns 2 to 4): the pane's `git status` split into
//! staged and unstaged files, the two versions of a file a diff is drawn
//! between, staging, and editing a file of the repository in place.
//!
//! The diff itself is computed by the frontend from the two versions, so the
//! "changes" and "whole file" views and the editor's gutter share one source.

use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

use serde::Serialize;
use tauri::{AppHandle, Manager};

use super::cli;
use crate::error::{AppError, Result};
use crate::pty::TerminalId;
use crate::triggers::feeds;

/// Reading a blob or staging a large file may take longer than a label lookup.
const TIMEOUT: Duration = Duration::from_secs(15);
/// Files above this size are listed but not diffed or edited.
const MAX_FILE_BYTES: usize = 2 * 1024 * 1024;
/// Bytes inspected for a NUL when deciding whether a file is binary, as git does.
const BINARY_SNIFF: usize = 8000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangedFile {
    /// Relative to the repository root, forward slashes.
    pub path: String,
    /// The source of a rename or copy.
    pub old_path: Option<String>,
    /// "M", "A", "D", "R", "C", "T", "?" (untracked) or "!" (conflict).
    pub status: String,
    /// None for binary files and for untracked files that are too large to count.
    pub additions: Option<u32>,
    pub deletions: Option<u32>,
    pub binary: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewStatus {
    pub repo_root: String,
    pub branch: Option<String>,
    /// Full commit id of HEAD, None in a repository without commits.
    pub head: Option<String>,
    pub head_short: Option<String>,
    pub staged: Vec<ChangedFile>,
    pub unstaged: Vec<ChangedFile>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileVersions {
    /// The side the diff starts from: HEAD for a staged file, the index for an
    /// unstaged one. None when the file does not exist there.
    pub old: Option<String>,
    /// The side the diff ends at: the index for a staged file, the working tree
    /// for an unstaged one. None when the file was deleted.
    pub new: Option<String>,
    /// The working tree copy, which is what the editor opens.
    pub disk: Option<String>,
    pub disk_mtime_ms: Option<u64>,
    pub binary: bool,
    pub too_large: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteResult {
    pub mtime_ms: u64,
}

// Status --------------------------------------------------------------------

/// One entry of `git status --porcelain=v1 -z`.
#[derive(Debug, PartialEq, Eq)]
struct StatusEntry {
    index: char,
    worktree: char,
    path: String,
    old_path: Option<String>,
}

fn parse_status(porcelain: &str) -> Vec<StatusEntry> {
    let mut entries = Vec::new();
    let mut fields = porcelain.split('\0');
    while let Some(field) = fields.next() {
        let mut chars = field.chars();
        let (Some(index), Some(worktree), Some(' ')) = (chars.next(), chars.next(), chars.next())
        else {
            continue;
        };
        let path = chars.as_str().to_string();
        // A rename or copy is followed by its source as a field of its own.
        let old_path = if matches!(index, 'R' | 'C') || matches!(worktree, 'R' | 'C') {
            fields.next().map(str::to_string)
        } else {
            None
        };
        entries.push(StatusEntry {
            index,
            worktree,
            path,
            old_path,
        });
    }
    entries
}

fn is_conflict(index: char, worktree: char) -> bool {
    matches!(
        (index, worktree),
        ('D', 'D') | ('A', 'U') | ('U', 'D') | ('U', 'A') | ('D', 'U') | ('A', 'A') | ('U', 'U')
    )
}

/// Line counts keyed by path, from `git diff --numstat -z`.
#[derive(Debug, PartialEq, Eq)]
struct NumStat {
    path: String,
    additions: Option<u32>,
    deletions: Option<u32>,
}

fn parse_numstat(output: &str) -> Vec<NumStat> {
    let mut stats = Vec::new();
    let mut fields = output.split('\0');
    while let Some(field) = fields.next() {
        let mut parts = field.splitn(3, '\t');
        let (Some(add), Some(del), Some(path)) = (parts.next(), parts.next(), parts.next()) else {
            continue;
        };
        // A rename leaves the path empty and follows with the source and target.
        let path = if path.is_empty() {
            let _source = fields.next();
            match fields.next() {
                Some(target) => target.to_string(),
                None => continue,
            }
        } else {
            path.to_string()
        };
        stats.push(NumStat {
            path,
            additions: add.parse().ok(),
            deletions: del.parse().ok(),
        });
    }
    stats
}

fn changed(entry: &StatusEntry, status: char, stats: &[NumStat]) -> ChangedFile {
    let stat = stats.iter().find(|stat| stat.path == entry.path);
    ChangedFile {
        path: entry.path.clone(),
        old_path: entry.old_path.clone(),
        status: status.to_string(),
        additions: stat.and_then(|stat| stat.additions),
        deletions: stat.and_then(|stat| stat.deletions),
        // numstat prints "-" for both counts of a binary file.
        binary: stat.is_some_and(|stat| stat.additions.is_none() && stat.deletions.is_none()),
    }
}

/// Splits porcelain entries into the two lists the review panel shows.
fn split_status(
    entries: &[StatusEntry],
    staged_stats: &[NumStat],
    unstaged_stats: &[NumStat],
) -> (Vec<ChangedFile>, Vec<ChangedFile>) {
    let mut staged = Vec::new();
    let mut unstaged = Vec::new();
    for entry in entries {
        if entry.index == '?' {
            unstaged.push(ChangedFile {
                status: "?".to_string(),
                ..changed(entry, '?', &[])
            });
            continue;
        }
        if entry.index == '!' {
            continue;
        }
        if is_conflict(entry.index, entry.worktree) {
            unstaged.push(changed(entry, '!', unstaged_stats));
            continue;
        }
        if entry.index != ' ' {
            staged.push(changed(entry, entry.index, staged_stats));
        }
        if entry.worktree != ' ' {
            // The worktree side of a staged rename is a change to the target.
            let unstaged_entry = StatusEntry {
                index: entry.index,
                worktree: entry.worktree,
                path: entry.path.clone(),
                old_path: None,
            };
            unstaged.push(changed(&unstaged_entry, entry.worktree, unstaged_stats));
        }
    }
    (staged, unstaged)
}

fn repo_root(cwd: &Path) -> Result<PathBuf> {
    cli::value(cwd, &["rev-parse", "--show-toplevel"])
        .map(PathBuf::from)
        .ok_or_else(|| AppError::NotFound {
            message: "this pane is not inside a git repository".to_string(),
        })
}

fn git_text(root: &Path, args: &[&str]) -> Result<String> {
    let output = cli::run_raw(root, args, TIMEOUT)?;
    if !output.ok {
        return Err(AppError::Io {
            message: format!("git {}: {}", args.join(" "), output.stderr.trim()),
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

pub fn status(cwd: &Path) -> Result<ReviewStatus> {
    let root = repo_root(cwd)?;
    let head = cli::value(&root, &["rev-parse", "--verify", "-q", "HEAD"]);
    let branch = cli::value(&root, &["symbolic-ref", "--short", "-q", "HEAD"]);

    let porcelain = git_text(
        &root,
        &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
    )?;
    let entries = parse_status(&porcelain);

    // Without a commit there is nothing for --cached to compare against except
    // the empty tree, which git spells as this well known id.
    let base = head
        .as_deref()
        .unwrap_or("4b825dc642cb6eb9a060e54bf8d69288fbee4904");
    let staged_stats = parse_numstat(&git_text(
        &root,
        &["diff", "--cached", "--numstat", "-z", base],
    )?);
    let unstaged_stats = parse_numstat(&git_text(&root, &["diff", "--numstat", "-z"])?);

    let (staged, mut unstaged) = split_status(&entries, &staged_stats, &unstaged_stats);
    for file in unstaged.iter_mut().filter(|file| file.status == "?") {
        let text = read_disk(&root, &file.path).ok().flatten();
        match text {
            Some(Blob::Text(text)) => {
                file.additions = Some(count_lines(&text));
                file.deletions = Some(0);
            }
            Some(Blob::Binary) => file.binary = true,
            _ => {}
        }
    }

    Ok(ReviewStatus {
        repo_root: normalise(&root),
        head_short: head.as_ref().map(|id| id.chars().take(7).collect()),
        head,
        branch,
        staged,
        unstaged,
    })
}

fn count_lines(text: &str) -> u32 {
    if text.is_empty() {
        return 0;
    }
    let newlines = text.bytes().filter(|byte| *byte == b'\n').count();
    (newlines + usize::from(!text.ends_with('\n'))) as u32
}

// Files -----------------------------------------------------------------------

/// Every file the edit tree lists (design turn 5): tracked files plus
/// untracked ones that are not ignored, relative to the root, sorted.
pub fn files(cwd: &Path) -> Result<Vec<String>> {
    let root = repo_root(cwd)?;
    let output = git_text(
        &root,
        &[
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
        ],
    )?;
    let mut paths: Vec<String> = output
        .split('\0')
        .filter(|path| !path.is_empty())
        .map(str::to_string)
        .collect();
    paths.sort();
    // A file with unmerged stages is listed once per stage.
    paths.dedup();
    Ok(paths)
}

// File versions ---------------------------------------------------------------

#[derive(Clone)]
enum Blob {
    Text(String),
    Binary,
    TooLarge,
}

fn classify(bytes: Vec<u8>) -> Blob {
    if bytes.len() > MAX_FILE_BYTES {
        return Blob::TooLarge;
    }
    if bytes[..bytes.len().min(BINARY_SNIFF)].contains(&0) {
        return Blob::Binary;
    }
    match String::from_utf8(bytes) {
        Ok(text) => Blob::Text(text),
        Err(_) => Blob::Binary,
    }
}

/// A blob out of git (`HEAD:path`, `:path`), None when it does not exist there.
fn read_object(root: &Path, spec: &str) -> Result<Option<Blob>> {
    let output = cli::run_raw(root, &["cat-file", "blob", spec], TIMEOUT)?;
    Ok(output.ok.then(|| classify(output.stdout)))
}

fn read_disk(root: &Path, rel_path: &str) -> Result<Option<Blob>> {
    let path = resolve(root, rel_path)?;
    if !path.is_file() {
        return Ok(None);
    }
    let size = fs::metadata(&path)?.len() as usize;
    if size > MAX_FILE_BYTES {
        return Ok(Some(Blob::TooLarge));
    }
    Ok(Some(classify(fs::read(&path)?)))
}

pub fn file_versions(
    cwd: &Path,
    rel_path: &str,
    old_path: Option<&str>,
    staged: bool,
) -> Result<FileVersions> {
    let root = repo_root(cwd)?;
    // Refuse anything outside the repository before handing it to git.
    resolve(&root, rel_path)?;
    if let Some(old_path) = old_path {
        resolve(&root, old_path)?;
    }
    let source = old_path.unwrap_or(rel_path);

    let disk = read_disk(&root, rel_path)?;
    let (old, new) = if staged {
        (
            read_object(&root, &format!("HEAD:{source}"))?,
            read_object(&root, &format!(":{rel_path}"))?,
        )
    } else {
        // An untracked file has no index entry, so it is new from nothing.
        (read_object(&root, &format!(":{rel_path}"))?, disk.clone())
    };

    let blobs = [&old, &new, &disk];
    let binary = blobs.iter().any(|blob| matches!(blob, Some(Blob::Binary)));
    let too_large = blobs
        .iter()
        .any(|blob| matches!(blob, Some(Blob::TooLarge)));
    let text = |blob: Option<Blob>| match blob {
        Some(Blob::Text(text)) => Some(text),
        _ => None,
    };

    Ok(FileVersions {
        disk_mtime_ms: matches!(disk, Some(Blob::Text(_))).then(|| mtime_ms(&root.join(rel_path))),
        old: text(old),
        new: text(new),
        disk: text(disk),
        binary,
        too_large,
    })
}

// Staging and editing --------------------------------------------------------------

fn check_paths(root: &Path, paths: &[String]) -> Result<()> {
    if paths.is_empty() {
        return Err(AppError::InvalidName {
            message: "no files given".to_string(),
        });
    }
    for path in paths {
        resolve(root, path)?;
    }
    Ok(())
}

fn run_paths(root: &Path, args: &[&str], paths: &[String]) -> Result<()> {
    // Literal pathspecs: a file named ":(glob)*" or "*.ts" is just that file.
    let mut full: Vec<&str> = vec!["--literal-pathspecs"];
    full.extend_from_slice(args);
    full.push("--");
    full.extend(paths.iter().map(String::as_str));
    git_text(root, &full).map(|_| ())
}

pub fn stage(cwd: &Path, paths: &[String]) -> Result<()> {
    let root = repo_root(cwd)?;
    check_paths(&root, paths)?;
    // -A so a file deleted on disk is staged as a deletion.
    run_paths(&root, &["add", "-A"], paths)
}

pub fn unstage(cwd: &Path, paths: &[String]) -> Result<()> {
    let root = repo_root(cwd)?;
    check_paths(&root, paths)?;
    if cli::value(&root, &["rev-parse", "--verify", "-q", "HEAD"]).is_some() {
        run_paths(&root, &["restore", "--staged"], paths)
    } else {
        // Before the first commit there is no HEAD to restore from.
        run_paths(
            &root,
            &["rm", "--cached", "-r", "-q", "--ignore-unmatch"],
            paths,
        )
    }
}

/// Writes the editor's buffer back to the working tree. Like a note, the
/// write is refused when the file changed on disk since it was read.
pub fn write_file(
    cwd: &Path,
    rel_path: &str,
    content: &str,
    expected_mtime_ms: Option<u64>,
) -> Result<WriteResult> {
    let root = repo_root(cwd)?;
    let path = resolve(&root, rel_path)?;

    if let Some(expected) = expected_mtime_ms {
        if path.exists() {
            let current = mtime_ms(&path);
            if current > expected + 1 {
                return Err(AppError::Conflict {
                    message: format!("{rel_path} changed on disk"),
                    current_mtime_ms: current,
                });
            }
        }
    }

    let parent = path.parent().ok_or_else(|| AppError::OutsideRoot {
        message: format!("{rel_path} has no parent directory"),
    })?;
    let temp = parent.join(format!(
        ".{}.tmp-{}",
        path.file_name().unwrap_or_default().to_string_lossy(),
        uuid::Uuid::new_v4()
    ));
    // Keep the file's permissions (an executable script stays executable).
    let permissions = fs::metadata(&path)
        .ok()
        .map(|metadata| metadata.permissions());
    fs::write(&temp, content)?;
    if let Some(permissions) = permissions {
        let _ = fs::set_permissions(&temp, permissions);
    }
    if let Err(error) = fs::rename(&temp, &path) {
        let _ = fs::remove_file(&temp);
        return Err(error.into());
    }
    Ok(WriteResult {
        mtime_ms: mtime_ms(&path),
    })
}

// Path helpers ------------------------------------------------------------------

/// Joins a repository relative path to the root and refuses anything that
/// leaves it: absolute paths, `..`, and symlinks pointing outside.
fn resolve(root: &Path, rel_path: &str) -> Result<PathBuf> {
    let outside = || AppError::OutsideRoot {
        message: format!("{rel_path} is outside the repository"),
    };
    let relative = Path::new(rel_path);
    if rel_path.is_empty() || relative.is_absolute() {
        return Err(outside());
    }
    let mut resolved = root.to_path_buf();
    for component in relative.components() {
        match component {
            Component::Normal(segment) => resolved.push(segment),
            Component::CurDir => {}
            _ => return Err(outside()),
        }
    }
    // Anything inside .git is git's business, not the editor's.
    if relative
        .components()
        .next()
        .is_some_and(|first| first.as_os_str() == ".git")
    {
        return Err(outside());
    }
    let canonical_root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    if let Ok(canonical) = resolved.canonicalize() {
        if !canonical.starts_with(&canonical_root) {
            return Err(outside());
        }
    }
    Ok(resolved)
}

fn normalise(path: &Path) -> String {
    super::worktree::normalise(&path.to_string_lossy())
}

fn mtime_ms(path: &Path) -> u64 {
    fs::metadata(path)
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|since| since.as_millis() as u64)
        .unwrap_or(0)
}

// Commands -------------------------------------------------------------------------

fn cwd_of(app: &AppHandle, id: &TerminalId) -> Result<PathBuf> {
    app.state::<crate::AppState>()
        .pty
        .cwd_of(id)
        .ok_or_else(|| AppError::NotFound {
            message: format!("terminal {id}"),
        })
}

/// The pane header's dirty marker follows staging and saving straight away.
fn refresh_labels(app: &AppHandle, id: &TerminalId) {
    let state = app.state::<crate::AppState>();
    super::refresh_repo_of(app, &state, id, Duration::ZERO);
}

#[tauri::command]
pub async fn review_status(app: AppHandle, id: TerminalId) -> Result<ReviewStatus> {
    crate::blocking(move || status(&cwd_of(&app, &id)?)).await?
}

#[tauri::command]
pub async fn review_files(app: AppHandle, id: TerminalId) -> Result<Vec<String>> {
    crate::blocking(move || files(&cwd_of(&app, &id)?)).await?
}

#[tauri::command]
pub async fn review_file(
    app: AppHandle,
    id: TerminalId,
    path: String,
    old_path: Option<String>,
    staged: bool,
) -> Result<FileVersions> {
    crate::blocking(move || file_versions(&cwd_of(&app, &id)?, &path, old_path.as_deref(), staged))
        .await?
}

#[tauri::command]
pub async fn review_stage(app: AppHandle, id: TerminalId, paths: Vec<String>) -> Result<()> {
    crate::blocking(move || {
        let cwd = cwd_of(&app, &id)?;
        stage(&cwd, &paths)?;
        refresh_labels(&app, &id);
        let state = app.state::<crate::AppState>();
        if let Some(repo) = feeds::repo_root_of(&state, &cwd) {
            feeds::publish(&app, feeds::review_staged(&repo, paths.len()));
        }
        Ok(())
    })
    .await?
}

#[tauri::command]
pub async fn review_unstage(app: AppHandle, id: TerminalId, paths: Vec<String>) -> Result<()> {
    crate::blocking(move || {
        unstage(&cwd_of(&app, &id)?, &paths)?;
        refresh_labels(&app, &id);
        Ok(())
    })
    .await?
}

#[tauri::command]
pub async fn review_write_file(
    app: AppHandle,
    id: TerminalId,
    path: String,
    content: String,
    expected_mtime_ms: Option<u64>,
) -> Result<WriteResult> {
    crate::blocking(move || {
        let result = write_file(&cwd_of(&app, &id)?, &path, &content, expected_mtime_ms)?;
        refresh_labels(&app, &id);
        Ok(result)
    })
    .await?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn porcelain_entries_keep_both_sides_and_rename_sources() {
        let entries = parse_status("M  src/a.ts\0 M src/b.ts\0R  new.ts\0old.ts\0?? notes.md\0");
        assert_eq!(entries.len(), 4);
        assert_eq!((entries[0].index, entries[0].worktree), ('M', ' '));
        assert_eq!((entries[1].index, entries[1].worktree), (' ', 'M'));
        assert_eq!(entries[2].path, "new.ts");
        assert_eq!(entries[2].old_path.as_deref(), Some("old.ts"));
        assert_eq!(entries[3].index, '?');
    }

    #[test]
    fn numstat_reads_counts_binaries_and_renames() {
        let stats = parse_numstat("4\t2\tsrc/a.ts\0-\t-\tlogo.png\x001\t0\t\0old.ts\0new.ts\0");
        assert_eq!(
            stats,
            vec![
                NumStat {
                    path: "src/a.ts".into(),
                    additions: Some(4),
                    deletions: Some(2)
                },
                NumStat {
                    path: "logo.png".into(),
                    additions: None,
                    deletions: None
                },
                NumStat {
                    path: "new.ts".into(),
                    additions: Some(1),
                    deletions: Some(0)
                },
            ]
        );
    }

    #[test]
    fn a_file_changed_on_both_sides_is_listed_twice() {
        let entries = parse_status("MM src/a.ts\0?? b.md\0UU c.ts\0");
        let staged_stats = parse_numstat("3\t1\tsrc/a.ts\0");
        let unstaged_stats = parse_numstat("1\t1\tsrc/a.ts\0");
        let (staged, unstaged) = split_status(&entries, &staged_stats, &unstaged_stats);
        assert_eq!(staged.len(), 1);
        assert_eq!(staged[0].additions, Some(3));
        let statuses: Vec<_> = unstaged.iter().map(|file| file.status.as_str()).collect();
        assert_eq!(statuses, vec!["M", "?", "!"]);
        assert_eq!(unstaged[0].additions, Some(1));
    }

    #[test]
    fn paths_outside_the_repository_are_refused() {
        let root = std::env::temp_dir();
        assert!(resolve(&root, "../etc/passwd").is_err());
        assert!(resolve(&root, "/etc/passwd").is_err());
        assert!(resolve(&root, ".git/config").is_err());
        assert!(resolve(&root, "").is_err());
        assert!(resolve(&root, "src/a.ts").is_ok());
    }

    #[test]
    fn line_counts_handle_a_missing_final_newline() {
        assert_eq!(count_lines(""), 0);
        assert_eq!(count_lines("a\nb\n"), 2);
        assert_eq!(count_lines("a\nb"), 2);
    }

    fn git(dir: &Path, args: &[&str]) {
        let status = Command::new("git")
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
    fn stage_edit_and_unstage_round_trip() {
        let dir = std::env::temp_dir().join(format!("tg-review-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        git(&dir, &["init", "-q", "-b", "main"]);
        fs::write(dir.join("a.txt"), "one\ntwo\n").unwrap();
        git(&dir, &["add", "a.txt"]);
        git(
            &dir,
            &["-c", "commit.gpgsign=false", "commit", "-q", "-m", "init"],
        );

        fs::write(dir.join("a.txt"), "one\n2\n").unwrap();
        fs::write(dir.join("new.md"), "hello\n").unwrap();
        let before = status(&dir).unwrap();
        assert!(before.staged.is_empty());
        assert_eq!(before.unstaged.len(), 2);
        assert_eq!(before.branch.as_deref(), Some("main"));

        stage(&dir, &["a.txt".to_string()]).unwrap();
        let staged = status(&dir).unwrap();
        assert_eq!(staged.staged.len(), 1);
        assert_eq!(staged.staged[0].additions, Some(1));
        assert_eq!(staged.staged[0].deletions, Some(1));

        let versions = file_versions(&dir, "a.txt", None, true).unwrap();
        assert_eq!(versions.old.as_deref(), Some("one\ntwo\n"));
        assert_eq!(versions.new.as_deref(), Some("one\n2\n"));

        let mtime = versions.disk_mtime_ms;
        write_file(&dir, "a.txt", "one\n2\nthree\n", mtime).unwrap();
        let edited = status(&dir).unwrap();
        assert!(edited.unstaged.iter().any(|file| file.path == "a.txt"));

        unstage(&dir, &["a.txt".to_string()]).unwrap();
        assert!(status(&dir).unwrap().staged.is_empty());

        fs::write(dir.join(".gitignore"), "build/\n").unwrap();
        fs::create_dir_all(dir.join("build")).unwrap();
        fs::write(dir.join("build").join("out.js"), "x\n").unwrap();
        assert_eq!(files(&dir).unwrap(), vec![".gitignore", "a.txt", "new.md"]);

        let _ = fs::remove_dir_all(&dir);
    }
}
