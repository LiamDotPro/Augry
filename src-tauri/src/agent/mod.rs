//! Agent finish detection. See docs/technical-design.md section 5.
//!
//! The primary signal is the process tree: once per second the watcher walks
//! the descendants of every shell and looks for a configured agent pattern in
//! the executable name or the command line. Claude Code on Windows runs as
//! `node.exe`, so matching the full command line is required.
//!
//! The same tick samples cpu and memory for the session's process tree, which
//! is what the pane header shows next to the agent badge, and picks up the
//! task line agents report through their task file (see `task`).

pub mod installed;
pub mod task;

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};
use tauri::{AppHandle, Emitter, Manager};

use crate::pty::{path_string, TerminalId};
use crate::triggers::feeds;
use crate::triggers::internal::InternalSignal;

pub const EVENT_AGENT: &str = "terminal://agent";
pub const EVENT_STATS: &str = "terminal://stats";

const TICK: Duration = Duration::from_millis(1000);
/// Filters out `claude --version` style invocations.
const MIN_RUNNING: Duration = Duration::from_millis(1500);
/// Guard against a pathological process tree.
const MAX_DEPTH: usize = 8;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentEvent {
    pub id: TerminalId,
    pub event: &'static str,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    pub source: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatsEvent {
    pub id: TerminalId,
    pub cpu_percent: f32,
    pub mem_bytes: u64,
}

/// Per session view of the process tree watcher.
#[derive(Debug, Clone, PartialEq)]
pub enum AgentState {
    Idle,
    Running {
        name: String,
        since: Instant,
        /// A `started` event is only sent once the run outlives MIN_RUNNING.
        announced: bool,
    },
}

#[derive(Debug, PartialEq)]
pub enum Transition {
    Started { name: String },
    Finished { name: String, duration_ms: u64 },
}

/// Advances one session's state for a tick. Pure, so the timing rules are
/// testable without a process tree.
pub fn step(
    state: &mut AgentState,
    detected: Option<&str>,
    now: Instant,
    min_running: Duration,
) -> Option<Transition> {
    match (&mut *state, detected) {
        (AgentState::Idle, Some(name)) => {
            *state = AgentState::Running {
                name: name.to_string(),
                since: now,
                announced: false,
            };
            None
        }
        (
            AgentState::Running {
                name,
                since,
                announced,
            },
            Some(detected),
        ) => {
            if *name != detected {
                // A different agent replaced the first one in the same shell.
                let previous = std::mem::replace(name, detected.to_string());
                let was_announced = std::mem::replace(announced, false);
                let elapsed = now.duration_since(*since);
                *since = now;
                return was_announced.then(|| Transition::Finished {
                    name: previous,
                    duration_ms: elapsed.as_millis() as u64,
                });
            }
            if !*announced && now.duration_since(*since) >= min_running {
                *announced = true;
                return Some(Transition::Started { name: name.clone() });
            }
            None
        }
        (
            AgentState::Running {
                name,
                since,
                announced,
            },
            None,
        ) => {
            let finished = announced.then(|| Transition::Finished {
                name: name.clone(),
                duration_ms: now.duration_since(*since).as_millis() as u64,
            });
            *state = AgentState::Idle;
            finished
        }
        (AgentState::Idle, None) => None,
    }
}

/// Returns the pattern that matched, so the badge shows what the user configured.
pub fn match_pattern<'a>(haystack: &str, patterns: &'a [String]) -> Option<&'a str> {
    let haystack = haystack.to_lowercase();
    patterns
        .iter()
        .find(|pattern| !pattern.is_empty() && haystack.contains(pattern.as_str()))
        .map(String::as_str)
}

struct Shared {
    wake: Mutex<bool>,
    signal: Condvar,
    stop: AtomicBool,
}

pub struct AgentWatcher {
    shared: Arc<Shared>,
}

impl Default for AgentWatcher {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentWatcher {
    pub fn new() -> Self {
        Self {
            shared: Arc::new(Shared {
                wake: Mutex::new(false),
                signal: Condvar::new(),
                stop: AtomicBool::new(false),
            }),
        }
    }

    /// Shortens the current sleep so a freshly created session is picked up now.
    pub fn wake(&self) {
        *self.shared.wake.lock().expect("agent wake") = true;
        self.shared.signal.notify_all();
    }

    pub fn stop(&self) {
        self.shared.stop.store(true, Ordering::SeqCst);
        self.wake();
    }

    /// Starts the polling thread. Called once from the Tauri setup hook.
    pub fn start(&self, app: AppHandle) {
        let shared = Arc::clone(&self.shared);
        std::thread::spawn(move || {
            let mut system = System::new();
            let mut states: HashMap<TerminalId, AgentState> = HashMap::new();
            let mut tasks = task::TaskTracker::default();

            while !shared.stop.load(Ordering::SeqCst) {
                {
                    let state: tauri::State<'_, crate::AppState> = app.state();
                    let shells = state.pty.shell_pids();
                    if !shells.is_empty() {
                        let patterns = state.state.config().agent_patterns;
                        tick(&app, &mut system, &mut states, &shells, &patterns);
                    }
                    states.retain(|id, _| shells.iter().any(|(shell_id, _)| shell_id == id));
                    tasks.poll(&app, &state.pty.task_files());
                }

                let mut woken = shared.wake.lock().expect("agent wake");
                let (guard, _) = shared.signal.wait_timeout(woken, TICK).expect("agent wake");
                woken = guard;
                *woken = false;
            }
        });
    }
}

fn tick(
    app: &AppHandle,
    system: &mut System,
    states: &mut HashMap<TerminalId, AgentState>,
    shells: &[(TerminalId, u32)],
    patterns: &[String],
) {
    // One refresh per tick, not one per session. Command lines are required or
    // the node.exe based agents never match.
    system.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing()
            .with_cmd(sysinfo::UpdateKind::Always)
            .with_exe(sysinfo::UpdateKind::Always)
            .with_cpu()
            .with_memory(),
    );

    let children = child_index(system);
    let now = Instant::now();

    for (id, shell_pid) in shells {
        let tree = descendants(&children, Pid::from_u32(*shell_pid));

        let mut detected: Option<String> = None;
        let mut cpu = 0.0f32;
        let mut mem = 0u64;

        for pid in &tree {
            let Some(process) = system.process(*pid) else {
                continue;
            };
            cpu += process.cpu_usage();
            mem += process.memory();

            if detected.is_none() {
                if let Some(matched) = match_pattern(&describe(process), patterns) {
                    detected = Some(matched.to_string());
                }
            }
        }

        let state = states.entry(id.clone()).or_insert(AgentState::Idle);
        if let Some(transition) = step(state, detected.as_deref(), now, MIN_RUNNING) {
            emit_transition(app, id, transition);
        }

        if !matches!(state, AgentState::Idle) {
            let _ = app.emit_to(
                "main",
                EVENT_STATS,
                StatsEvent {
                    id: id.clone(),
                    cpu_percent: cpu,
                    mem_bytes: mem,
                },
            );
        }
    }
}

fn emit_transition(app: &AppHandle, id: &TerminalId, transition: Transition) {
    let finished = finished_agent(&transition).map(str::to_string);
    let event = match transition {
        Transition::Started { name } => AgentEvent {
            id: id.clone(),
            event: "started",
            name,
            duration_ms: None,
            source: "process",
        },
        Transition::Finished { name, duration_ms } => AgentEvent {
            id: id.clone(),
            event: "finished",
            name,
            duration_ms: Some(duration_ms),
            source: "process",
        },
    };
    let _ = app.emit_to("main", EVENT_AGENT, event);

    // This runs on the watcher thread, so only in memory lookups happen here:
    // the pane's directory is a map read, never a git call.
    if let Some(agent) = finished {
        let cwd = app
            .state::<crate::AppState>()
            .pty
            .cwd_of(id)
            .map(|cwd| path_string(&cwd))
            .unwrap_or_default();
        feeds::publish(
            app,
            InternalSignal::AgentFinished {
                pane: id.clone(),
                cwd,
                agent,
            },
        );
    }
}

/// The agent's name when the transition is a finish; a start names nobody.
fn finished_agent(transition: &Transition) -> Option<&str> {
    match transition {
        Transition::Finished { name, .. } => Some(name),
        Transition::Started { .. } => None,
    }
}

/// Executable name plus full command line, lowercased by the matcher.
fn describe(process: &sysinfo::Process) -> String {
    let name = process.name().to_string_lossy().into_owned();
    let args: Vec<String> = process
        .cmd()
        .iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    format!("{name} {}", args.join(" "))
}

/// parent pid -> children, built once per tick instead of rescanning per session.
fn child_index(system: &System) -> HashMap<Pid, Vec<Pid>> {
    let mut index: HashMap<Pid, Vec<Pid>> = HashMap::new();
    for (pid, process) in system.processes() {
        if let Some(parent) = process.parent() {
            index.entry(parent).or_default().push(*pid);
        }
    }
    index
}

/// The shell and everything below it, breadth first and depth limited.
fn descendants(children: &HashMap<Pid, Vec<Pid>>, root: Pid) -> Vec<Pid> {
    let mut found = vec![root];
    let mut frontier = vec![root];

    for _ in 0..MAX_DEPTH {
        let mut next = Vec::new();
        for pid in frontier.drain(..) {
            for child in children.get(&pid).into_iter().flatten() {
                if !found.contains(child) {
                    found.push(*child);
                    next.push(*child);
                }
            }
        }
        if next.is_empty() {
            break;
        }
        frontier = next;
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN: Duration = Duration::from_millis(1500);

    #[test]
    fn a_short_run_never_announces_anything() {
        let start = Instant::now();
        let mut state = AgentState::Idle;
        assert_eq!(step(&mut state, Some("claude"), start, MIN), None);
        // Gone again before the minimum running time: no started, no finished.
        assert_eq!(
            step(&mut state, None, start + Duration::from_millis(400), MIN),
            None
        );
        assert_eq!(state, AgentState::Idle);
    }

    #[test]
    fn a_real_run_announces_start_then_finish() {
        let start = Instant::now();
        let mut state = AgentState::Idle;
        step(&mut state, Some("claude"), start, MIN);

        let announced = step(
            &mut state,
            Some("claude"),
            start + Duration::from_secs(2),
            MIN,
        );
        assert_eq!(
            announced,
            Some(Transition::Started {
                name: "claude".to_string()
            })
        );

        let finished = step(&mut state, None, start + Duration::from_secs(9), MIN);
        match finished {
            Some(Transition::Finished { name, duration_ms }) => {
                assert_eq!(name, "claude");
                assert!(duration_ms >= 9000);
            }
            other => panic!("expected a finish, got {other:?}"),
        }
        assert_eq!(state, AgentState::Idle);
    }

    #[test]
    fn start_is_announced_only_once() {
        let start = Instant::now();
        let mut state = AgentState::Idle;
        step(&mut state, Some("claude"), start, MIN);
        step(
            &mut state,
            Some("claude"),
            start + Duration::from_secs(2),
            MIN,
        );
        assert_eq!(
            step(
                &mut state,
                Some("claude"),
                start + Duration::from_secs(3),
                MIN
            ),
            None
        );
    }

    #[test]
    fn swapping_agents_finishes_the_first_one() {
        let start = Instant::now();
        let mut state = AgentState::Idle;
        step(&mut state, Some("claude"), start, MIN);
        step(
            &mut state,
            Some("claude"),
            start + Duration::from_secs(2),
            MIN,
        );

        let swapped = step(
            &mut state,
            Some("codex"),
            start + Duration::from_secs(4),
            MIN,
        );
        assert!(matches!(
            swapped,
            Some(Transition::Finished { ref name, .. }) if name == "claude"
        ));
        assert!(matches!(state, AgentState::Running { ref name, .. } if name == "codex"));
    }

    #[test]
    fn only_a_finish_names_an_agent_for_the_trigger_feed() {
        let started = Transition::Started {
            name: "claude".to_string(),
        };
        let finished = Transition::Finished {
            name: "claude".to_string(),
            duration_ms: 9000,
        };
        assert_eq!(finished_agent(&started), None);
        assert_eq!(finished_agent(&finished), Some("claude"));
    }

    #[test]
    fn matching_looks_at_the_whole_command_line() {
        let patterns = vec!["claude".to_string(), "codex".to_string()];
        assert_eq!(
            match_pattern(r"node.exe C:\npm\Claude\cli.js --resume", &patterns),
            Some("claude")
        );
        assert_eq!(match_pattern("pwsh.exe -NoLogo", &patterns), None);
    }

    #[test]
    fn descendants_walks_the_tree_and_stops_at_the_depth_limit() {
        let mut children: HashMap<Pid, Vec<Pid>> = HashMap::new();
        let pid = Pid::from_u32;
        children.insert(pid(1), vec![pid(2), pid(3)]);
        children.insert(pid(2), vec![pid(4)]);

        let tree = descendants(&children, pid(1));
        assert_eq!(tree.len(), 4);
        assert!(tree.contains(&pid(4)));

        // A cycle must not loop forever.
        let mut cyclic: HashMap<Pid, Vec<Pid>> = HashMap::new();
        cyclic.insert(pid(1), vec![pid(2)]);
        cyclic.insert(pid(2), vec![pid(1)]);
        assert_eq!(descendants(&cyclic, pid(1)).len(), 2);
    }
}
