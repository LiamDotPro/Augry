//! Your processes, as files in the app's config folder.
//!
//! One process is up to three files in `<config dir>/processes/`:
//! - `<id>.json`: the latest saved version, the file you can open and read;
//! - `<id>.history.jsonl`: every saved version, one JSON object per line, oldest first;
//! - `<id>.draft.json`: unsaved edits, when there are any.
//!
//! The Process tab owns the format (src/process/types.ts); this module only
//! keeps the files. Team processes live in the cloud and never come here.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Manager};

use crate::error::{AppError, Result};
use crate::AppState;

const HISTORY: &str = ".history.jsonl";
const DRAFT: &str = ".draft.json";
const FILE: &str = ".json";

pub struct ProcessStore {
    dir: PathBuf,
}

/// A process as found on disk. `problem` says why a file couldn't be read;
/// the rest is whatever could be.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StoredProcess {
    pub id: String,
    /// Saved versions, oldest first.
    pub versions: Vec<Value>,
    pub draft: Option<Value>,
    /// `<id>.json` as it is now; it differs from the latest version when it
    /// was edited outside the app.
    pub file: Option<Value>,
    pub problem: Option<String>,
}

impl ProcessStore {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    fn path(&self, id: &str, suffix: &str) -> PathBuf {
        self.dir.join(format!("{id}{suffix}"))
    }

    /// Every process in the folder, sorted by id.
    pub fn load(&self) -> Result<Vec<StoredProcess>> {
        fs::create_dir_all(&self.dir)?;
        let mut ids = std::collections::BTreeSet::new();
        for entry in fs::read_dir(&self.dir)? {
            let name = entry?.file_name().to_string_lossy().into_owned();
            // Longest suffix first: "a.draft.json" is a draft, not a file called "a.draft".
            let id = [HISTORY, DRAFT, FILE].iter().find_map(|suffix| name.strip_suffix(suffix));
            if let Some(id) = id.filter(|id| check_id(id).is_ok()) {
                ids.insert(id.to_string());
            }
        }
        Ok(ids.into_iter().map(|id| self.read(&id)).collect())
    }

    fn read(&self, id: &str) -> StoredProcess {
        let mut problems = Vec::new();
        let mut versions = Vec::new();
        match fs::read_to_string(self.path(id, HISTORY)) {
            Ok(text) => {
                for (index, line) in text.lines().enumerate() {
                    if line.trim().is_empty() {
                        continue;
                    }
                    match serde_json::from_str::<Value>(line) {
                        Ok(value) => versions.push(value),
                        Err(error) => problems.push(format!("history line {}: {error}", index + 1)),
                    }
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => problems.push(format!("history: {error}")),
        }
        let file = read_json(&self.path(id, FILE), &mut problems);
        let draft = read_json(&self.path(id, DRAFT), &mut problems);
        StoredProcess {
            id: id.to_string(),
            versions,
            draft,
            file,
            problem: (!problems.is_empty()).then(|| problems.join("; ")),
        }
    }

    /// Adds a saved version: appends it to the history and makes it the file.
    /// The draft goes, since it is now saved.
    pub fn save_version(&self, id: &str, version: &Value) -> Result<()> {
        check_id(id)?;
        let process = version.get("process").ok_or_else(|| AppError::InvalidName {
            message: "a version needs its process".to_string(),
        })?;
        fs::create_dir_all(&self.dir)?;
        let line = serde_json::to_string(version).map_err(io_error)?;
        let mut history = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.path(id, HISTORY))?;
        writeln!(history, "{line}")?;
        history.sync_all()?;
        write_atomic(&self.path(id, FILE), &pretty(process)?)?;
        remove_if_there(&self.path(id, DRAFT))
    }

    /// Keeps unsaved edits, or clears them with None.
    pub fn save_draft(&self, id: &str, draft: Option<&Value>) -> Result<()> {
        check_id(id)?;
        match draft {
            Some(draft) => {
                fs::create_dir_all(&self.dir)?;
                write_atomic(&self.path(id, DRAFT), &pretty(draft)?)
            }
            None => remove_if_there(&self.path(id, DRAFT)),
        }
    }

    /// Moves a process's files to the trash, so a mistake can be undone there.
    pub fn delete(&self, id: &str) -> Result<()> {
        check_id(id)?;
        for suffix in [FILE, HISTORY, DRAFT] {
            let path = self.path(id, suffix);
            if path.exists() {
                trash::delete(&path).map_err(|error| AppError::Io {
                    message: error.to_string(),
                })?;
            }
        }
        Ok(())
    }

    pub fn file_of(&self, id: &str) -> Result<PathBuf> {
        check_id(id)?;
        Ok(self.path(id, FILE))
    }
}

/// Process ids are file names: letters, digits, `-` and `_`, starting with a
/// letter or digit. Nothing that could leave the folder.
fn check_id(id: &str) -> Result<()> {
    let ok = !id.is_empty()
        && id.len() <= 100
        && id.chars().next().is_some_and(|c| c.is_ascii_alphanumeric())
        && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if ok {
        Ok(())
    } else {
        Err(AppError::InvalidName {
            message: format!("not a process id: {id}"),
        })
    }
}

fn read_json(path: &Path, problems: &mut Vec<String>) -> Option<Value> {
    match fs::read_to_string(path) {
        Ok(text) => match serde_json::from_str(&text) {
            Ok(value) => Some(value),
            Err(error) => {
                let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                problems.push(format!("{name}: {error}"));
                None
            }
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            problems.push(error.to_string());
            None
        }
    }
}

fn pretty(value: &Value) -> Result<String> {
    serde_json::to_string_pretty(value).map_err(io_error)
}

fn io_error(error: serde_json::Error) -> AppError {
    AppError::Io {
        message: error.to_string(),
    }
}

/// Through a temp file, so a crash mid-write can't truncate the real one.
fn write_atomic(path: &Path, text: &str) -> Result<()> {
    let temp = path.with_extension("tmp");
    fs::write(&temp, text)?;
    fs::rename(&temp, path)?;
    Ok(())
}

fn remove_if_there(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error.into()),
        _ => Ok(()),
    }
}

fn store(app: &AppHandle) -> &ProcessStore {
    &app.state::<AppState>().inner().processes
}

// Commands ----------------------------------------------------------------

#[tauri::command]
pub async fn processes_load(app: AppHandle) -> Result<Vec<StoredProcess>> {
    crate::blocking(move || store(&app).load()).await?
}

#[tauri::command]
pub async fn process_save_version(app: AppHandle, id: String, version: Value) -> Result<()> {
    crate::blocking(move || store(&app).save_version(&id, &version)).await?
}

#[tauri::command]
pub async fn process_save_draft(app: AppHandle, id: String, draft: Option<Value>) -> Result<()> {
    crate::blocking(move || store(&app).save_draft(&id, draft.as_ref())).await?
}

#[tauri::command]
pub async fn process_delete(app: AppHandle, id: String) -> Result<()> {
    crate::blocking(move || store(&app).delete(&id)).await?
}

/// Opens `<id>.json` in the app the system uses for JSON files.
#[tauri::command]
pub fn process_open_file(app: AppHandle, id: String) -> Result<()> {
    let path = store(&app).file_of(&id)?;
    tauri_plugin_opener::open_path(&path, None::<&str>).map_err(|error| AppError::Io {
        message: error.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn temp_store() -> (ProcessStore, PathBuf) {
        let dir = std::env::temp_dir().join(format!("augry-processes-{}", uuid::Uuid::new_v4()));
        (ProcessStore::new(dir.clone()), dir)
    }

    fn version(n: u64, name: &str) -> Value {
        json!({ "version": n, "message": "Saved", "by": "You", "at": 1, "hash": "abc", "process": { "id": "bug-fix", "name": name } })
    }

    #[test]
    fn saves_versions_and_reads_them_back() {
        let (store, dir) = temp_store();
        store.save_version("bug-fix", &version(1, "Bug fix")).unwrap();
        store.save_draft("bug-fix", Some(&json!({ "name": "Bug fix draft" }))).unwrap();
        store.save_version("bug-fix", &version(2, "Bug fix 2")).unwrap();

        let loaded = store.load().unwrap();
        assert_eq!(loaded.len(), 1);
        let process = &loaded[0];
        assert_eq!(process.id, "bug-fix");
        assert_eq!(process.versions.len(), 2);
        assert_eq!(process.file, Some(json!({ "id": "bug-fix", "name": "Bug fix 2" })));
        // Saving clears the draft.
        assert_eq!(process.draft, None);
        assert_eq!(process.problem, None);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn keeps_and_clears_a_draft() {
        let (store, dir) = temp_store();
        store.save_version("a", &version(1, "A")).unwrap();
        store.save_draft("a", Some(&json!({ "name": "A2" }))).unwrap();
        let loaded = store.load().unwrap();
        assert_eq!(loaded.len(), 1, "the draft file is the same process, not another one");
        assert_eq!(loaded[0].draft, Some(json!({ "name": "A2" })));
        store.save_draft("a", None).unwrap();
        assert_eq!(store.load().unwrap()[0].draft, None);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn reports_a_broken_file_without_losing_the_rest() {
        let (store, dir) = temp_store();
        store.save_version("a", &version(1, "A")).unwrap();
        fs::write(dir.join("a.json"), "{ not json").unwrap();
        let loaded = store.load().unwrap();
        assert_eq!(loaded[0].versions.len(), 1);
        assert_eq!(loaded[0].file, None);
        assert!(loaded[0].problem.as_deref().unwrap_or("").starts_with("a.json"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn refuses_ids_that_could_leave_the_folder() {
        let (store, dir) = temp_store();
        for id in ["", "../x", "a/b", ".hidden", "a b", "a.draft"] {
            assert!(store.save_version(id, &version(1, "x")).is_err(), "{id}");
        }
        let _ = fs::remove_dir_all(dir);
    }
}
