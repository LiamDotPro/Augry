//! Agent task reports. See docs/technical-design.md section 5.5.
//!
//! Every shell is started with `TERMINAL_GRID_TASK_FILE` pointing at a file of
//! its own. An agent (or anything else in the pane) writes one short line
//! there describing what it is working on; the agent watcher stats the files
//! on its tick and emits `terminal://task` when one changes. A file is the
//! lowest common denominator: every agent can run `echo ... > "$VAR"`, it needs
//! no port or token, and agents that capture their tools' output (so an OSC
//! sequence would never reach the terminal) still get through.

use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::pty::{path_string, TerminalId};
use crate::triggers::feeds;

pub const EVENT_TASK: &str = "terminal://task";
pub const ENV_TASK_FILE: &str = "TERMINAL_GRID_TASK_FILE";

/// Anything past this is not a one line status and is not read at all.
const MAX_READ_BYTES: u64 = 4096;
/// What the pane overlay shows; longer reports are cut with an ellipsis.
const MAX_TASK_CHARS: usize = 280;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskEvent {
    pub id: TerminalId,
    /// `None` when the file was emptied, which clears the task.
    pub task: Option<String>,
    pub updated_at_ms: u64,
}

pub fn task_file(dir: &Path, id: &TerminalId) -> PathBuf {
    dir.join(format!("{id}.txt"))
}

/// Creates the file empty, so a restarted session never shows the last run's task.
pub fn reset_task_file(path: &Path) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, b"")
}

/// Files left behind by a previous run belong to sessions that no longer exist.
pub fn clear_task_dir(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "txt") {
            let _ = fs::remove_file(path);
        }
    }
}

/// Turns the raw file into the one line the overlay shows.
///
/// Windows PowerShell 5.1 writes `>` redirects as UTF-16LE with a BOM, so that
/// is decoded as well as UTF-8. Whitespace (newlines included) collapses to
/// single spaces.
pub fn parse_task(raw: &[u8]) -> Option<String> {
    let text = decode(raw);
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        return None;
    }
    if collapsed.chars().count() <= MAX_TASK_CHARS {
        return Some(collapsed);
    }
    let cut: String = collapsed.chars().take(MAX_TASK_CHARS - 1).collect();
    Some(format!("{}…", cut.trim_end()))
}

fn decode(raw: &[u8]) -> String {
    if let Some(rest) = raw.strip_prefix(&[0xFF, 0xFE]) {
        let units: Vec<u16> = rest
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        return String::from_utf16_lossy(&units);
    }
    if let Some(rest) = raw.strip_prefix(&[0xFE, 0xFF]) {
        let units: Vec<u16> = rest
            .chunks_exact(2)
            .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
            .collect();
        return String::from_utf16_lossy(&units);
    }
    let raw = raw.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(raw);
    String::from_utf8_lossy(raw).into_owned()
}

/// Modification time plus length, so a same-second rewrite still registers.
type Stamp = (SystemTime, u64);

/// Remembers what each session's file looked like on the last tick.
#[derive(Default)]
pub struct TaskTracker {
    seen: HashMap<TerminalId, Stamp>,
}

impl TaskTracker {
    pub fn poll(&mut self, app: &AppHandle, files: &[(TerminalId, PathBuf)]) {
        self.seen
            .retain(|id, _| files.iter().any(|(file_id, _)| file_id == id));

        for (id, path) in files {
            let Ok(meta) = fs::metadata(path) else {
                continue;
            };
            let stamp = (meta.modified().unwrap_or(UNIX_EPOCH), meta.len());
            let previous = self.seen.insert(id.clone(), stamp);
            if previous == Some(stamp) {
                continue;
            }
            // The empty file written at spawn is not news.
            if previous.is_none() && meta.len() == 0 {
                continue;
            }

            let task = read_capped(path).and_then(|raw| parse_task(&raw));
            let updated_at_ms = stamp
                .0
                .duration_since(UNIX_EPOCH)
                .map(|elapsed| elapsed.as_millis() as u64)
                .unwrap_or(0);
            let reported = task.clone();
            let _ = app.emit_to(
                "main",
                EVENT_TASK,
                TaskEvent {
                    id: id.clone(),
                    task,
                    updated_at_ms,
                },
            );

            // Only a map read, because this is the agent watcher thread.
            let cwd = app
                .state::<crate::AppState>()
                .pty
                .cwd_of(id)
                .map(|cwd| path_string(&cwd))
                .unwrap_or_default();
            if let Some(signal) = feeds::task_reported(id, &cwd, reported.as_deref()) {
                feeds::publish(app, signal);
            }
        }
    }
}

fn read_capped(path: &Path) -> Option<Vec<u8>> {
    let file = fs::File::open(path).ok()?;
    let mut raw = Vec::new();
    file.take(MAX_READ_BYTES).read_to_end(&mut raw).ok()?;
    Some(raw)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_line_comes_through_trimmed() {
        assert_eq!(
            parse_task(b"  Refactoring the auth middleware\n").as_deref(),
            Some("Refactoring the auth middleware")
        );
    }

    #[test]
    fn an_empty_or_blank_file_clears_the_task() {
        assert_eq!(parse_task(b""), None);
        assert_eq!(parse_task(b" \r\n\t"), None);
    }

    #[test]
    fn newlines_collapse_into_one_line() {
        assert_eq!(
            parse_task(b"Fixing tests\r\n  step 2 of 3\n").as_deref(),
            Some("Fixing tests step 2 of 3")
        );
    }

    #[test]
    fn utf16_from_windows_powershell_is_decoded() {
        let mut raw = vec![0xFF, 0xFE];
        for unit in "Écrire les tests\r\n".encode_utf16() {
            raw.extend_from_slice(&unit.to_le_bytes());
        }
        assert_eq!(parse_task(&raw).as_deref(), Some("Écrire les tests"));
    }

    #[test]
    fn a_utf8_bom_is_dropped() {
        assert_eq!(
            parse_task(b"\xEF\xBB\xBFWiring IPC").as_deref(),
            Some("Wiring IPC")
        );
    }

    #[test]
    fn long_reports_are_cut_with_an_ellipsis() {
        let long = "x".repeat(MAX_TASK_CHARS * 2);
        let task = parse_task(long.as_bytes()).expect("a task");
        assert_eq!(task.chars().count(), MAX_TASK_CHARS);
        assert!(task.ends_with('…'));
    }
}
