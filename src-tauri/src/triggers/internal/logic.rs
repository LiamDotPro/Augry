//! Which armed rules a signal fires, and with what payload. Pure: no threads,
//! no clock, no channels. `InternalSource::publish` sends what this returns.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::json;
use trigger_core::{RuleId, Value};

use super::rule::{Filter, Parsed};
use super::InternalSignal;

#[cfg(test)]
mod tests;

/// The longest `line` in a `tg.output-matched` payload, in characters.
pub const MAX_LINE_CHARS: usize = 200;

/// A rule that passed `rule::parse`, ready to be matched.
#[derive(Debug, Clone)]
pub struct Armed {
    pub id: RuleId,
    pub kind: String,
    pub parsed: Parsed,
}

/// One rule that fired: its id, its kind and the payload.
pub type Fire = (RuleId, String, BTreeMap<String, Value>);

/// The rules this signal fires, in the order of `armed`.
pub fn step(armed: &[Armed], signal: &InternalSignal) -> Vec<Fire> {
    let kinds = kinds_of(signal);
    // A pane that opened outside a repository has nothing to fire.
    if kinds.is_empty() {
        return Vec::new();
    }
    let place = place_of(signal);
    let payload = payload_of(signal);
    armed
        .iter()
        .filter(|a| kinds.contains(&a.kind.as_str()))
        .filter(|a| repo_ok(a.parsed.repo.as_deref(), place))
        .filter(|a| filter_ok(&a.parsed.filter, signal))
        .map(|a| (a.id.clone(), a.kind.clone(), payload.clone()))
        .collect()
}

/// The kinds a signal can fire. One command end can be both a failure and a
/// slow command.
fn kinds_of(signal: &InternalSignal) -> &'static [&'static str] {
    use InternalSignal::*;
    match signal {
        AgentFinished { .. } => &["tg.agent-finished"],
        CommandEnded { .. } => &["tg.command-failed", "tg.command-slow"],
        OutputLine { .. } => &["tg.output-matched"],
        Bell { .. } => &["tg.bell"],
        TaskReported { .. } => &["tg.task-reported"],
        PaneOpened {
            repo_root: Some(_), ..
        } => &["tg.pane-opened-in-repo"],
        PaneOpened {
            repo_root: None, ..
        } => &[],
        WorktreeCreated { .. } => &["tg.worktree-created"],
        WorktreeRemoved { .. } => &["tg.worktree-removed"],
        ReviewStaged { .. } => &["tg.review-staged"],
        NoteSaved { .. } => &["tg.note-saved"],
        FocusEntered { .. } => &["tg.focus-entered"],
        AppStarted => &["tg.app-started"],
    }
}

/// The path a rule's `repo` is compared with: the pane's directory, or the
/// repository the signal names. None for signals that have no place.
fn place_of(signal: &InternalSignal) -> Option<&str> {
    use InternalSignal::*;
    match signal {
        AgentFinished { cwd, .. }
        | CommandEnded { cwd, .. }
        | OutputLine { cwd, .. }
        | TaskReported { cwd, .. }
        | FocusEntered { cwd, .. } => Some(cwd),
        PaneOpened { repo_root, .. } => repo_root.as_deref(),
        WorktreeCreated { repo, .. } | WorktreeRemoved { repo, .. } | ReviewStaged { repo, .. } => {
            Some(repo)
        }
        Bell { .. } | NoteSaved { .. } | AppStarted => None,
    }
}

/// No `repo` on the rule matches everything. Otherwise the place must be the
/// repository or a folder inside it.
fn repo_ok(repo: Option<&Path>, place: Option<&str>) -> bool {
    match (repo, place) {
        (None, _) => true,
        (Some(repo), Some(place)) => Path::new(place).starts_with(repo),
        (Some(_), None) => false,
    }
}

fn filter_ok(filter: &Filter, signal: &InternalSignal) -> bool {
    use InternalSignal::*;
    match (filter, signal) {
        (Filter::None, _) => true,
        (Filter::Agent(glob), AgentFinished { agent, .. }) => {
            glob.as_ref().is_none_or(|g| g.is_match(agent))
        }
        (Filter::Failed { exit_code }, CommandEnded { exit_code: got, .. }) => match exit_code {
            Some(wanted) => i64::from(*got) == *wanted,
            None => *got != 0,
        },
        (
            Filter::Slow {
                longer_than_ms,
                exit_code,
            },
            CommandEnded {
                exit_code: got,
                duration_ms,
                ..
            },
        ) => {
            duration_ms > longer_than_ms && exit_code.is_none_or(|wanted| i64::from(*got) == wanted)
        }
        (Filter::Pattern(regex), OutputLine { line, .. }) => regex.is_match(line),
        (Filter::Note(glob), NoteSaved { relative, .. }) => {
            glob.as_ref().is_none_or(|g| g.is_match(relative))
        }
        _ => false,
    }
}

fn payload_of(signal: &InternalSignal) -> BTreeMap<String, Value> {
    use InternalSignal::*;
    let pairs: Vec<(&str, Value)> = match signal {
        AgentFinished { pane, cwd, agent } => {
            vec![
                ("pane", json!(pane)),
                ("cwd", json!(cwd)),
                ("agent", json!(agent)),
            ]
        }
        CommandEnded {
            pane,
            cwd,
            exit_code,
            duration_ms,
        } => vec![
            ("pane", json!(pane)),
            ("cwd", json!(cwd)),
            ("exit_code", json!(exit_code)),
            ("duration_ms", json!(duration_ms)),
        ],
        OutputLine { pane, cwd, line } => {
            let line: String = line.chars().take(MAX_LINE_CHARS).collect();
            vec![
                ("pane", json!(pane)),
                ("cwd", json!(cwd)),
                ("line", json!(line)),
            ]
        }
        Bell { pane } => vec![("pane", json!(pane))],
        TaskReported { pane, cwd } | FocusEntered { pane, cwd } => {
            vec![("pane", json!(pane)), ("cwd", json!(cwd))]
        }
        PaneOpened {
            pane,
            cwd,
            repo_root,
        } => {
            let mut pairs = vec![("pane", json!(pane)), ("cwd", json!(cwd))];
            if let Some(root) = repo_root {
                pairs.push(("repo", json!(root)));
            }
            pairs
        }
        WorktreeCreated { repo, path, branch } => {
            let mut pairs = vec![("repo", json!(repo)), ("path", json!(path))];
            if let Some(branch) = branch {
                pairs.push(("branch", json!(branch)));
            }
            pairs
        }
        WorktreeRemoved { repo, path } => vec![("repo", json!(repo)), ("path", json!(path))],
        ReviewStaged { repo, files } => vec![("repo", json!(repo)), ("files", json!(files))],
        NoteSaved { path, relative } => vec![("path", json!(path)), ("relative", json!(relative))],
        AppStarted => Vec::new(),
    };
    pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect()
}
