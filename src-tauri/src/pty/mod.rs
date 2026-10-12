//! Terminal sessions. See docs/technical-design.md section 4.

pub mod marks;
pub mod reader;
pub mod shell;
pub mod tap;

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use base64::Engine as _;
use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, MasterPty, PtySize};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::agent::task;
use crate::error::{AppError, Result};
use crate::pty::reader::OutputPump;
use crate::pty::shell::ShellKind;

pub type TerminalId = String;

pub const EVENT_OUTPUT: &str = "terminal://output";
pub const EVENT_EXIT: &str = "terminal://exit";
pub const EVENT_CWD: &str = "terminal://cwd";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalInfo {
    pub id: TerminalId,
    pub cwd: String,
    pub shell: ShellKind,
    pub shell_pid: Option<u32>,
    pub exited: bool,
    pub exit_code: Option<i32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputEvent {
    pub id: TerminalId,
    pub data_b64: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExitEvent {
    pub id: TerminalId,
    pub code: Option<i32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CwdEvent {
    pub id: TerminalId,
    pub cwd: String,
}

struct Session {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    killer: Box<dyn ChildKiller + Send + Sync>,
    shell_pid: Option<u32>,
    shell: ShellKind,
    cwd: PathBuf,
    exited: bool,
    exit_code: Option<i32>,
    #[allow(dead_code)]
    spawned_at: Instant,
    /// Bumped on restart so the previous reader and waiter threads fall silent.
    generation: u64,
}

impl Session {
    fn info(&self, id: &TerminalId) -> TerminalInfo {
        TerminalInfo {
            id: id.clone(),
            cwd: path_string(&self.cwd),
            shell: self.shell,
            shell_pid: self.shell_pid,
            exited: self.exited,
            exit_code: self.exit_code,
        }
    }
}

#[derive(Default)]
pub struct PtyManager {
    sessions: Arc<Mutex<HashMap<TerminalId, Session>>>,
    focused: Mutex<Option<TerminalId>>,
    script_dir: Mutex<PathBuf>,
    generations: Arc<AtomicU64>,
}

impl PtyManager {
    pub fn new(script_dir: PathBuf) -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
            focused: Mutex::new(None),
            script_dir: Mutex::new(script_dir),
            generations: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn cwd_of(&self, id: &TerminalId) -> Option<PathBuf> {
        self.sessions
            .lock()
            .expect("pty sessions")
            .get(id)
            .map(|session| session.cwd.clone())
    }

    /// Every live session's working directory, for the worktree cross reference.
    pub fn open_dirs(&self) -> Vec<(TerminalId, PathBuf)> {
        self.sessions
            .lock()
            .expect("pty sessions")
            .iter()
            .map(|(id, session)| (id.clone(), session.cwd.clone()))
            .collect()
    }

    /// Every live session's task file, for the agent watcher's task poll.
    pub fn task_files(&self) -> Vec<(TerminalId, PathBuf)> {
        let dir = self.tasks_dir();
        self.sessions
            .lock()
            .expect("pty sessions")
            .iter()
            .filter(|(_, session)| !session.exited)
            .map(|(id, _)| (id.clone(), task::task_file(&dir, id)))
            .collect()
    }

    /// Where the per session task files live, next to the integration scripts.
    pub fn tasks_dir(&self) -> PathBuf {
        self.script_dir
            .lock()
            .expect("pty script dir")
            .join("tasks")
    }

    /// Live shell pids, for the agent watcher's process tree walk.
    pub fn shell_pids(&self) -> Vec<(TerminalId, u32)> {
        self.sessions
            .lock()
            .expect("pty sessions")
            .iter()
            .filter(|(_, session)| !session.exited)
            .filter_map(|(id, session)| session.shell_pid.map(|pid| (id.clone(), pid)))
            .collect()
    }

    pub fn ids(&self) -> Vec<TerminalId> {
        self.sessions
            .lock()
            .expect("pty sessions")
            .keys()
            .cloned()
            .collect()
    }

    pub fn focused(&self) -> Option<TerminalId> {
        self.focused.lock().expect("pty focus").clone()
    }

    pub fn set_focused(&self, id: Option<TerminalId>) {
        *self.focused.lock().expect("pty focus") = id;
    }

    pub fn list(&self) -> Vec<TerminalInfo> {
        self.sessions
            .lock()
            .expect("pty sessions")
            .iter()
            .map(|(id, session)| session.info(id))
            .collect()
    }

    pub fn set_script_dir(&self, dir: PathBuf) {
        *self.script_dir.lock().expect("pty script dir") = dir;
    }

    /// Spawns a shell and starts its reader and waiter threads.
    pub fn spawn(
        &self,
        app: &AppHandle,
        id: TerminalId,
        cwd: PathBuf,
        cols: u16,
        rows: u16,
        config: &crate::state::Config,
    ) -> Result<TerminalInfo> {
        let script_dir = self.script_dir.lock().expect("pty script dir").clone();
        let shell = shell::resolve(config.shell.as_deref(), &config.shell_args, &script_dir);

        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows: rows.max(1),
                cols: cols.max(1),
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|error| AppError::SpawnFailed {
                message: error.to_string(),
            })?;

        let mut command = CommandBuilder::new(&shell.program);
        for arg in &shell.args {
            command.arg(arg);
        }
        for (key, value) in &shell.env {
            command.env(key, value);
        }
        command.cwd(&cwd);
        command.env("TERM", "xterm-256color");
        command.env("COLORTERM", "truecolor");
        command.env("TERMINAL_GRID", "1");
        command.env("TERMINAL_GRID_ID", &id);

        // A task file the pane's agent can report into (agent::task). A pane
        // without one still works, it just never shows a task.
        let task_path = task::task_file(&self.tasks_dir(), &id);
        match task::reset_task_file(&task_path) {
            Ok(()) => command.env(task::ENV_TASK_FILE, path_string(&task_path)),
            Err(error) => eprintln!("terminal-grid: no task file for {id}: {error}"),
        }

        let child = pair
            .slave
            .spawn_command(command)
            .map_err(|error| AppError::SpawnFailed {
                message: format!("{}: {error}", shell.program),
            })?;

        // The slave must be dropped or the reader never sees EOF on exit.
        drop(pair.slave);

        let shell_pid = child.process_id();
        let killer = child.clone_killer();
        let output = pair
            .master
            .try_clone_reader()
            .map_err(|error| AppError::SpawnFailed {
                message: error.to_string(),
            })?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|error| AppError::SpawnFailed {
                message: error.to_string(),
            })?;

        let generation = self.generations.fetch_add(1, Ordering::SeqCst) + 1;

        let session = Session {
            master: pair.master,
            writer,
            killer,
            shell_pid,
            shell: shell.kind,
            cwd,
            exited: false,
            exit_code: None,
            spawned_at: Instant::now(),
            generation,
        };
        let info = session.info(&id);
        self.sessions
            .lock()
            .expect("pty sessions")
            .insert(id.clone(), session);

        self.start_reader(app.clone(), id.clone(), generation, output);
        self.start_waiter(app.clone(), id, generation, child);

        Ok(info)
    }

    /// Two threads per session: one blocks on the pty and fills the pump, the
    /// other drains it and is the only one that emits, so output stays ordered
    /// and a lone prompt still reaches the frontend (see `reader`).
    fn start_reader(
        &self,
        app: AppHandle,
        id: TerminalId,
        generation: u64,
        mut output: Box<dyn Read + Send>,
    ) {
        let pump = Arc::new(OutputPump::new());
        let sessions = Arc::clone(&self.sessions);

        {
            let pump = Arc::clone(&pump);
            let id = id.clone();
            std::thread::spawn(move || {
                let mut buffer = [0u8; reader::READ_BUFFER];
                loop {
                    match output.read(&mut buffer) {
                        Ok(0) => break,
                        Ok(count) => {
                            pump.push(&buffer[..count]);
                            // Pace a runaway writer rather than dropping its bytes.
                            if pump.should_throttle() {
                                std::thread::sleep(reader::THROTTLE_SLEEP);
                            }
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                        Err(_) => break,
                    }

                    if !is_current(&sessions, &id, generation) {
                        break;
                    }
                }
                pump.close();
            });
        }

        std::thread::spawn(move || {
            while let Some(batch) = pump.next_batch() {
                if !emit_output(&app, &id, batch) {
                    break;
                }
            }
        });
    }

    fn start_waiter(
        &self,
        app: AppHandle,
        id: TerminalId,
        generation: u64,
        mut child: Box<dyn portable_pty::Child + Send + Sync>,
    ) {
        let sessions = Arc::clone(&self.sessions);
        std::thread::spawn(move || {
            let code = child.wait().ok().map(|status| status.exit_code() as i32);

            let mut guard = sessions.lock().expect("pty sessions");
            match guard.get_mut(&id) {
                // A restart replaced the session; the new one owns the id now.
                Some(session) if session.generation != generation => return,
                Some(session) => {
                    session.exited = true;
                    session.exit_code = code;
                }
                None => return,
            }
            drop(guard);

            let _ = app.emit_to("main", EVENT_EXIT, ExitEvent { id, code });
        });
    }

    pub fn write(&self, id: &TerminalId, bytes: &[u8]) -> Result<()> {
        let mut guard = self.sessions.lock().expect("pty sessions");
        let session = guard.get_mut(id).ok_or_else(|| not_found(id))?;
        if session.exited {
            return Ok(());
        }
        session.writer.write_all(bytes)?;
        session.writer.flush()?;
        Ok(())
    }

    pub fn resize(&self, id: &TerminalId, cols: u16, rows: u16) -> Result<()> {
        let guard = self.sessions.lock().expect("pty sessions");
        let session = guard.get(id).ok_or_else(|| not_found(id))?;
        session
            .master
            .resize(PtySize {
                rows: rows.max(1),
                cols: cols.max(1),
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|error| AppError::Io {
                message: error.to_string(),
            })
    }

    pub fn set_cwd(&self, id: &TerminalId, cwd: PathBuf) -> Result<bool> {
        let mut guard = self.sessions.lock().expect("pty sessions");
        let session = guard.get_mut(id).ok_or_else(|| not_found(id))?;
        if session.cwd == cwd {
            return Ok(false);
        }
        session.cwd = cwd;
        Ok(true)
    }

    /// Kills the shell and forgets the session.
    pub fn close(&self, id: &TerminalId) -> Result<()> {
        let mut guard = self.sessions.lock().expect("pty sessions");
        let Some(mut session) = guard.remove(id) else {
            return Ok(());
        };
        drop(guard);
        let _ = std::fs::remove_file(task::task_file(&self.tasks_dir(), id));

        // Killing the ConPTY closes its children too, so agents get a chance to
        // clean up. The waiter thread reaps the process; nothing here blocks the
        // command, and the pane is gone from the map either way.
        if !session.exited {
            let _ = session.killer.kill();
        }
        Ok(())
    }

    /// Respawns in the last known cwd, keeping the id so the pane stays put.
    pub fn restart(
        &self,
        app: &AppHandle,
        id: &TerminalId,
        config: &crate::state::Config,
    ) -> Result<TerminalInfo> {
        let (cwd, cols, rows) = {
            let mut guard = self.sessions.lock().expect("pty sessions");
            let session = guard.get_mut(id).ok_or_else(|| not_found(id))?;
            let size = session.master.get_size().ok();
            if !session.exited {
                let _ = session.killer.kill();
            }
            (
                session.cwd.clone(),
                size.map(|s| s.cols).unwrap_or(80),
                size.map(|s| s.rows).unwrap_or(24),
            )
        };

        let cwd = if cwd.is_dir() { cwd } else { home_dir() };
        self.sessions.lock().expect("pty sessions").remove(id);
        self.spawn(app, id.clone(), cwd, cols, rows, config)
    }

    pub fn close_all(&self) {
        let mut guard = self.sessions.lock().expect("pty sessions");
        for (_, session) in guard.iter_mut() {
            if !session.exited {
                let _ = session.killer.kill();
            }
        }
        guard.clear();
    }

    pub fn has_live_sessions(&self) -> bool {
        self.sessions
            .lock()
            .expect("pty sessions")
            .values()
            .any(|session| !session.exited)
    }
}

fn is_current(
    sessions: &Arc<Mutex<HashMap<TerminalId, Session>>>,
    id: &TerminalId,
    generation: u64,
) -> bool {
    sessions
        .lock()
        .expect("pty sessions")
        .get(id)
        .is_some_and(|session| session.generation == generation)
}

fn emit_output(app: &AppHandle, id: &TerminalId, bytes: Vec<u8>) -> bool {
    let data_b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
    app.emit_to(
        "main",
        EVENT_OUTPUT,
        OutputEvent {
            id: id.clone(),
            data_b64,
        },
    )
    .is_ok()
}

fn not_found(id: &TerminalId) -> AppError {
    AppError::NotFound {
        message: format!("no terminal {id}"),
    }
}

/// Forward slashes everywhere, which is what the frontend and git both use.
pub fn path_string(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

pub fn home_dir() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

// Commands ----------------------------------------------------------------

#[tauri::command]
pub async fn create_terminal(
    app: AppHandle,
    cwd: Option<String>,
    cols: u16,
    rows: u16,
) -> Result<TerminalInfo> {
    crate::blocking(move || {
        let requested = cwd.map(PathBuf::from).unwrap_or_else(home_dir);
        // A saved directory that no longer exists must not stop the session from
        // starting (design section 9).
        let cwd = if requested.is_dir() {
            requested
        } else {
            home_dir()
        };

        let state = app.state::<crate::AppState>();
        let id = uuid::Uuid::new_v4().to_string();
        let config = state.state.config();
        let info = state.pty.spawn(&app, id, cwd, cols, rows, &config)?;
        state.agent.wake();
        crate::triggers::refresh_app(&app);
        Ok(info)
    })
    .await?
}

#[tauri::command]
pub fn write_terminal(
    state: State<'_, crate::AppState>,
    id: TerminalId,
    data: String,
) -> Result<()> {
    state.pty.write(&id, data.as_bytes())
}

#[tauri::command]
pub fn write_terminal_bytes(
    state: State<'_, crate::AppState>,
    id: TerminalId,
    data_b64: String,
) -> Result<()> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data_b64)
        .map_err(|error| AppError::Io {
            message: error.to_string(),
        })?;
    state.pty.write(&id, &bytes)
}

#[tauri::command]
pub fn resize_terminal(
    state: State<'_, crate::AppState>,
    id: TerminalId,
    cols: u16,
    rows: u16,
) -> Result<()> {
    state.pty.resize(&id, cols, rows)
}

#[tauri::command]
pub fn close_terminal(
    app: AppHandle,
    state: State<'_, crate::AppState>,
    id: TerminalId,
) -> Result<()> {
    state.pty.close(&id)?;
    crate::triggers::refresh_app(&app);
    Ok(())
}

#[tauri::command]
pub async fn restart_terminal(app: AppHandle, id: TerminalId) -> Result<TerminalInfo> {
    crate::blocking(move || {
        let state = app.state::<crate::AppState>();
        let config = state.state.config();
        let info = state.pty.restart(&app, &id, &config)?;
        state.agent.wake();
        Ok(info)
    })
    .await?
}

#[tauri::command]
pub fn list_terminals(state: State<'_, crate::AppState>) -> Vec<TerminalInfo> {
    state.pty.list()
}

#[tauri::command]
pub fn set_terminal_cwd(
    app: AppHandle,
    state: State<'_, crate::AppState>,
    id: TerminalId,
    cwd: String,
) -> Result<()> {
    report_cwd(app, state, id, cwd)
}

/// Called from the frontend's OSC 7 handler. A changed directory refreshes git
/// straight away (design section 7.4).
#[tauri::command]
pub fn report_cwd(
    app: AppHandle,
    state: State<'_, crate::AppState>,
    id: TerminalId,
    cwd: String,
) -> Result<()> {
    let path = PathBuf::from(&cwd);
    if !state.pty.set_cwd(&id, path)? {
        return Ok(());
    }

    let _ = app.emit_to(
        "main",
        EVENT_CWD,
        CwdEvent {
            id: id.clone(),
            cwd: cwd.clone(),
        },
    );
    crate::triggers::refresh_app(&app);

    let app_handle = app.clone();
    let inner_id = id;
    tauri::async_runtime::spawn_blocking(move || {
        let state: tauri::State<'_, crate::AppState> = app_handle.state();
        crate::git::refresh_repo_of(&app_handle, &state, &inner_id, std::time::Duration::ZERO);
    });
    Ok(())
}

#[tauri::command]
pub fn set_focused_terminal(
    app: AppHandle,
    state: State<'_, crate::AppState>,
    id: Option<TerminalId>,
) -> Result<()> {
    state.pty.set_focused(id.clone());
    let Some(id) = id else {
        return Ok(());
    };

    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state: tauri::State<'_, crate::AppState> = app_handle.state();
        crate::git::refresh_repo_of(&app_handle, &state, &id, crate::git::FOCUS_MAX_AGE);
    });
    Ok(())
}
