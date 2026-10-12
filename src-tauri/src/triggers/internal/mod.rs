//! The `tg` source: triggers the app raises itself (an agent finished, a
//! command failed, a worktree created). Built up over several cards; the
//! finished module doc and `README.md` come with the last ones.

/// Something the app saw. The feeds build one and hand it to
/// `TriggerService::publish_internal`.
#[derive(Debug, Clone, PartialEq)]
pub enum InternalSignal {
    AgentFinished {
        pane: String,
        cwd: String,
        agent: String,
    },
    CommandEnded {
        pane: String,
        cwd: String,
        exit_code: i32,
        duration_ms: u64,
    },
    OutputLine {
        pane: String,
        cwd: String,
        line: String,
    },
    Bell {
        pane: String,
    },
    TaskReported {
        pane: String,
        cwd: String,
    },
    PaneOpened {
        pane: String,
        cwd: String,
        repo_root: Option<String>,
    },
    WorktreeCreated {
        repo: String,
        path: String,
        branch: Option<String>,
    },
    WorktreeRemoved {
        repo: String,
        path: String,
    },
    ReviewStaged {
        repo: String,
        files: u32,
    },
    NoteSaved {
        path: String,
        relative: String,
    },
    FocusEntered {
        pane: String,
        cwd: String,
    },
    AppStarted,
}
