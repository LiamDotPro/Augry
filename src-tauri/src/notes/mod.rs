//! Markdown notes under a sandboxed root. See docs/technical-design.md section 8.

pub mod paths;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::error::{AppError, Result};

pub const EVENT_NOTES_CHANGED: &str = "notes://changed";
const DEBOUNCE: Duration = Duration::from_millis(300);
const EXTENSION: &str = "md";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NodeKind {
    File,
    Folder,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteNode {
    pub name: String,
    pub rel_path: String,
    pub kind: NodeKind,
    pub mtime_ms: u64,
    pub size: u64,
    pub children: Option<Vec<NoteNode>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotesChangedEvent {
    pub rel_path: String,
    pub kind: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteContent {
    pub content: String,
    pub mtime_ms: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteResult {
    pub mtime_ms: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RootResult {
    pub root: String,
}

pub struct NotesService {
    root: RwLock<PathBuf>,
    watcher: Mutex<Option<RecommendedWatcher>>,
}

impl NotesService {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root: RwLock::new(root),
            watcher: Mutex::new(None),
        }
    }

    /// `%USERPROFILE%\Documents\TerminalGrid\Notes` unless configured otherwise.
    pub fn default_root() -> PathBuf {
        dirs::document_dir()
            .unwrap_or_else(crate::pty::home_dir)
            .join("TerminalGrid")
            .join("Notes")
    }

    pub fn root(&self) -> PathBuf {
        self.root.read().expect("notes root").clone()
    }

    pub fn set_root(&self, root: PathBuf) -> Result<PathBuf> {
        fs::create_dir_all(&root)?;
        let canonical = root.canonicalize().unwrap_or(root);
        *self.root.write().expect("notes root") = canonical.clone();
        Ok(canonical)
    }

    fn resolve(&self, rel_path: &str) -> Result<PathBuf> {
        paths::resolve(&self.root(), rel_path)
    }

    /// Folders first, then files, both sorted case insensitively. Only `.md`
    /// files and directories are listed; hidden entries are skipped.
    pub fn tree(&self) -> Result<Vec<NoteNode>> {
        let root = self.root();
        fs::create_dir_all(&root)?;
        read_dir(&root, &root)
    }

    pub fn read(&self, rel_path: &str) -> Result<NoteContent> {
        let path = self.resolve(rel_path)?;
        let content = fs::read_to_string(&path).map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => AppError::NotFound {
                message: format!("{rel_path} does not exist"),
            },
            _ => AppError::Io {
                message: error.to_string(),
            },
        })?;
        Ok(NoteContent {
            mtime_ms: mtime_ms(&path),
            content,
        })
    }

    /// Atomic: written to a sibling temp file and renamed over the target, so a
    /// crash mid-write cannot leave a half saved note.
    pub fn write(
        &self,
        rel_path: &str,
        content: &str,
        expected_mtime_ms: Option<u64>,
    ) -> Result<WriteResult> {
        let path = self.resolve(rel_path)?;

        if let Some(expected) = expected_mtime_ms {
            if path.exists() {
                let current = mtime_ms(&path);
                // Filesystem timestamps are coarse; only a clear difference counts.
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
        fs::create_dir_all(parent)?;

        let temp = parent.join(format!(
            ".{}.tmp-{}",
            path.file_name().unwrap_or_default().to_string_lossy(),
            uuid::Uuid::new_v4()
        ));
        fs::write(&temp, content)?;
        if let Err(error) = fs::rename(&temp, &path) {
            let _ = fs::remove_file(&temp);
            return Err(error.into());
        }

        Ok(WriteResult {
            mtime_ms: mtime_ms(&path),
        })
    }

    pub fn create(&self, rel_path: &str, kind: NodeKind) -> Result<NoteNode> {
        let rel_path = normalise_rel(rel_path, kind);
        let path = self.resolve(&rel_path)?;
        if path.exists() {
            return Err(AppError::InvalidName {
                message: format!("{rel_path} already exists"),
            });
        }

        match kind {
            NodeKind::Folder => fs::create_dir_all(&path)?,
            NodeKind::File => {
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent)?;
                }
                // Open the editor on something rather than a blank buffer.
                let title = path
                    .file_stem()
                    .map(|stem| stem.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "Untitled".to_string());
                fs::write(&path, format!("# {title}\n"))?;
            }
        }

        node_for(&self.root(), &path, kind)
    }

    pub fn rename(&self, rel_path: &str, new_rel_path: &str) -> Result<NoteNode> {
        let from = self.resolve(rel_path)?;
        let kind = if from.is_dir() {
            NodeKind::Folder
        } else {
            NodeKind::File
        };
        let new_rel_path = normalise_rel(new_rel_path, kind);
        let to = self.resolve(&new_rel_path)?;

        if !from.exists() {
            return Err(AppError::NotFound {
                message: format!("{rel_path} does not exist"),
            });
        }
        if to.exists() && to != from {
            return Err(AppError::InvalidName {
                message: format!("{new_rel_path} already exists"),
            });
        }
        if let Some(parent) = to.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::rename(&from, &to)?;
        node_for(&self.root(), &to, kind)
    }

    /// Goes to the Recycle Bin, never a hard delete (design section 8.3).
    pub fn delete(&self, rel_path: &str, force: bool) -> Result<()> {
        let path = self.resolve(rel_path)?;
        if !path.exists() {
            return Err(AppError::NotFound {
                message: format!("{rel_path} does not exist"),
            });
        }
        if path.is_dir() && !force {
            let empty = fs::read_dir(&path)?.next().is_none();
            if !empty {
                return Err(AppError::Conflict {
                    message: format!("{rel_path} is not empty"),
                    current_mtime_ms: mtime_ms(&path),
                });
            }
        }
        trash::delete(&path).map_err(|error| AppError::Io {
            message: error.to_string(),
        })
    }

    pub fn reveal(&self, app: &AppHandle, rel_path: &str) -> Result<()> {
        let path = self.resolve(rel_path)?;
        tauri_plugin_opener::reveal_item_in_dir(&path).map_err(|error| AppError::Io {
            message: error.to_string(),
        })?;
        let _ = app;
        Ok(())
    }

    /// Watches the root recursively and reports changes, debounced so a save
    /// that touches a temp file and then renames only fires once.
    pub fn watch(&self, app: AppHandle) -> Result<()> {
        let root = self.root();
        fs::create_dir_all(&root)?;

        let last: Arc<Mutex<Option<(String, std::time::Instant)>>> = Arc::new(Mutex::new(None));
        let watch_root = root.clone();

        let mut watcher =
            notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                let Ok(event) = event else { return };
                let kind = match event.kind {
                    notify::EventKind::Create(_) => "create",
                    notify::EventKind::Remove(_) => "remove",
                    notify::EventKind::Modify(notify::event::ModifyKind::Name(_)) => "rename",
                    notify::EventKind::Modify(_) => "modify",
                    _ => return,
                };

                for path in event.paths {
                    if is_temp(&path) {
                        continue;
                    }
                    let Some(rel_path) = paths::relative_of(&watch_root, &path) else {
                        continue;
                    };
                    if !path.is_dir() && !rel_path.to_lowercase().ends_with(".md") {
                        continue;
                    }

                    let mut guard = last.lock().expect("notes debounce");
                    let now = std::time::Instant::now();
                    if let Some((previous, at)) = guard.as_ref() {
                        if previous == &rel_path && now.duration_since(*at) < DEBOUNCE {
                            continue;
                        }
                    }
                    *guard = Some((rel_path.clone(), now));
                    drop(guard);

                    let _ = app.emit_to(
                        "main",
                        EVENT_NOTES_CHANGED,
                        NotesChangedEvent { rel_path, kind },
                    );
                }
            })
            .map_err(|error| AppError::Io {
                message: error.to_string(),
            })?;

        watcher
            .watch(&root, RecursiveMode::Recursive)
            .map_err(|error| AppError::Io {
                message: error.to_string(),
            })?;

        // Dropping the watcher would stop it, so it is parked on the service.
        *self.watcher.lock().expect("notes watcher") = Some(watcher);
        Ok(())
    }
}

// Helpers -----------------------------------------------------------------

fn read_dir(root: &Path, dir: &Path) -> Result<Vec<NoteNode>> {
    let mut folders = Vec::new();
    let mut files = Vec::new();

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }

        let Some(rel_path) = paths::relative_of(root, &path) else {
            continue;
        };
        let metadata = match entry.metadata() {
            Ok(metadata) => metadata,
            Err(_) => continue,
        };

        if metadata.is_dir() {
            folders.push(NoteNode {
                name,
                rel_path,
                kind: NodeKind::Folder,
                mtime_ms: metadata_mtime_ms(&metadata),
                size: 0,
                children: Some(read_dir(root, &path)?),
            });
        } else if has_md_extension(&path) {
            files.push(NoteNode {
                name,
                rel_path,
                kind: NodeKind::File,
                mtime_ms: metadata_mtime_ms(&metadata),
                size: metadata.len(),
                children: None,
            });
        }
    }

    let by_name = |a: &NoteNode, b: &NoteNode| a.name.to_lowercase().cmp(&b.name.to_lowercase());
    folders.sort_by(by_name);
    files.sort_by(by_name);
    folders.append(&mut files);
    Ok(folders)
}

fn node_for(root: &Path, path: &Path, kind: NodeKind) -> Result<NoteNode> {
    let metadata = fs::metadata(path)?;
    Ok(NoteNode {
        name: path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default(),
        rel_path: paths::relative_of(root, path).unwrap_or_default(),
        kind,
        mtime_ms: metadata_mtime_ms(&metadata),
        size: metadata.len(),
        children: matches!(kind, NodeKind::Folder).then(Vec::new),
    })
}

/// New notes always end up as `.md`; folders keep their name as given.
fn normalise_rel(rel_path: &str, kind: NodeKind) -> String {
    let trimmed = rel_path.trim().trim_matches('/');
    if matches!(kind, NodeKind::File) && !trimmed.to_lowercase().ends_with(".md") {
        format!("{trimmed}.{EXTENSION}")
    } else {
        trimmed.to_string()
    }
}

fn has_md_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case(EXTENSION))
}

fn is_temp(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with('.') || name.contains(".tmp-"))
}

fn mtime_ms(path: &Path) -> u64 {
    fs::metadata(path)
        .map(|metadata| metadata_mtime_ms(&metadata))
        .unwrap_or(0)
}

fn metadata_mtime_ms(metadata: &fs::Metadata) -> u64 {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|since| since.as_millis() as u64)
        .unwrap_or_else(|| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|since| since.as_millis() as u64)
                .unwrap_or(0)
        })
}

// Commands ----------------------------------------------------------------

#[tauri::command]
pub fn notes_get_root(state: State<'_, crate::AppState>) -> RootResult {
    RootResult {
        root: crate::pty::path_string(&state.notes.root()),
    }
}

#[tauri::command]
pub async fn notes_set_root(app: AppHandle, root: String) -> Result<RootResult> {
    crate::blocking(move || {
        let state = app.state::<crate::AppState>();
        let resolved = state.notes.set_root(PathBuf::from(root))?;
        state
            .state
            .update_config(|config| config.notes_root = Some(resolved.clone()))?;
        // Re-point the filesystem watcher at the new folder.
        state.notes.watch(app.clone())?;
        Ok(RootResult {
            root: crate::pty::path_string(&resolved),
        })
    })
    .await?
}

#[tauri::command]
pub async fn notes_tree(app: AppHandle) -> Result<Vec<NoteNode>> {
    crate::blocking(move || app.state::<crate::AppState>().notes.tree()).await?
}

#[tauri::command]
pub async fn notes_read(app: AppHandle, rel_path: String) -> Result<NoteContent> {
    crate::blocking(move || app.state::<crate::AppState>().notes.read(&rel_path)).await?
}

#[tauri::command]
pub async fn notes_write(
    app: AppHandle,
    rel_path: String,
    content: String,
    expected_mtime_ms: Option<u64>,
) -> Result<WriteResult> {
    crate::blocking(move || {
        let state = app.state::<crate::AppState>();
        let result = state.notes.write(&rel_path, &content, expected_mtime_ms)?;
        let signal = crate::triggers::feeds::note_saved(&state.notes.root(), &rel_path);
        crate::triggers::feeds::publish(&app, signal);
        Ok(result)
    })
    .await?
}

#[tauri::command]
pub async fn notes_create(app: AppHandle, rel_path: String, kind: NodeKind) -> Result<NoteNode> {
    crate::blocking(move || app.state::<crate::AppState>().notes.create(&rel_path, kind)).await?
}

#[tauri::command]
pub async fn notes_rename(
    app: AppHandle,
    rel_path: String,
    new_rel_path: String,
) -> Result<NoteNode> {
    crate::blocking(move || {
        app.state::<crate::AppState>()
            .notes
            .rename(&rel_path, &new_rel_path)
    })
    .await?
}

#[tauri::command]
pub async fn notes_delete(app: AppHandle, rel_path: String, force: Option<bool>) -> Result<()> {
    crate::blocking(move || {
        app.state::<crate::AppState>()
            .notes
            .delete(&rel_path, force.unwrap_or(false))
    })
    .await?
}

#[tauri::command]
pub async fn notes_reveal(app: AppHandle, rel_path: String) -> Result<()> {
    crate::blocking(move || {
        let state = app.state::<crate::AppState>();
        state.notes.reveal(&app, &rel_path)
    })
    .await?
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempRoot(PathBuf);

    impl TempRoot {
        fn new(name: &str) -> Self {
            let path =
                std::env::temp_dir().join(format!("terminal-grid-{name}-{}", uuid::Uuid::new_v4()));
            fs::create_dir_all(&path).expect("temp root");
            Self(path)
        }

        fn service(&self) -> NotesService {
            NotesService::new(self.0.clone())
        }
    }

    impl Drop for TempRoot {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn creating_a_file_seeds_it_with_a_title() {
        let root = TempRoot::new("create");
        let notes = root.service();
        let node = notes.create("ideas", NodeKind::Folder).expect("folder");
        assert_eq!(node.kind, NodeKind::Folder);

        let node = notes.create("ideas/kafka", NodeKind::File).expect("file");
        assert_eq!(node.rel_path, "ideas/kafka.md");
        assert_eq!(
            notes.read("ideas/kafka.md").expect("read").content,
            "# kafka\n"
        );
    }

    #[test]
    fn the_tree_lists_folders_first_and_skips_non_markdown() {
        let root = TempRoot::new("tree");
        let notes = root.service();
        notes.create("zeta", NodeKind::Folder).expect("folder");
        notes.create("alpha.md", NodeKind::File).expect("file");
        fs::write(root.0.join("ignored.txt"), "x").expect("write");
        fs::write(root.0.join(".hidden.md"), "x").expect("write");

        let tree = notes.tree().expect("tree");
        assert_eq!(tree.len(), 2);
        assert_eq!(tree[0].name, "zeta");
        assert_eq!(tree[0].kind, NodeKind::Folder);
        assert_eq!(tree[1].name, "alpha.md");
    }

    #[test]
    fn writing_with_a_stale_mtime_reports_a_conflict() {
        let root = TempRoot::new("conflict");
        let notes = root.service();
        notes.create("note.md", NodeKind::File).expect("file");
        let current = notes.read("note.md").expect("read").mtime_ms;

        let stale = current.saturating_sub(60_000);
        match notes.write("note.md", "mine", Some(stale)) {
            Err(AppError::Conflict {
                current_mtime_ms, ..
            }) => {
                assert!(current_mtime_ms >= current);
            }
            other => panic!("expected a conflict, got {other:?}"),
        }

        // No guard means last write wins.
        assert!(notes.write("note.md", "mine", None).is_ok());
        assert_eq!(notes.read("note.md").expect("read").content, "mine");
    }

    #[test]
    fn writing_leaves_no_temp_file_behind() {
        let root = TempRoot::new("atomic");
        let notes = root.service();
        notes.create("note.md", NodeKind::File).expect("file");
        notes.write("note.md", "content", None).expect("write");

        let leftovers: Vec<_> = fs::read_dir(&root.0)
            .expect("read dir")
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.contains(".tmp-"))
            .collect();
        assert!(leftovers.is_empty(), "left behind {leftovers:?}");
    }

    #[test]
    fn renaming_moves_the_file_and_reports_the_new_path() {
        let root = TempRoot::new("rename");
        let notes = root.service();
        notes.create("old.md", NodeKind::File).expect("file");
        let node = notes.rename("old.md", "new.md").expect("rename");
        assert_eq!(node.rel_path, "new.md");
        assert!(notes.read("old.md").is_err());
    }

    #[test]
    fn a_non_empty_folder_needs_force_to_delete() {
        let root = TempRoot::new("delete");
        let notes = root.service();
        notes.create("box", NodeKind::Folder).expect("folder");
        notes.create("box/note.md", NodeKind::File).expect("file");
        assert!(matches!(
            notes.delete("box", false),
            Err(AppError::Conflict { .. })
        ));
    }

    #[test]
    fn commands_cannot_reach_outside_the_root() {
        let root = TempRoot::new("sandbox");
        let notes = root.service();
        assert!(matches!(
            notes.read("../../secrets.md"),
            Err(AppError::OutsideRoot { .. })
        ));
        assert!(matches!(
            notes.write("C:/Windows/system.ini", "x", None),
            Err(AppError::OutsideRoot { .. })
        ));
    }
}
