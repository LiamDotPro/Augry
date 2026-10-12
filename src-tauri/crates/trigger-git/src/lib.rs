//! Fires when something happens in a git repository. See README.md.
//!
//! Glue only: the kinds this crate fires, the `Source` implementation, and the
//! thread that feeds OS signals through the pure logic into the sink. Rules
//! live in `rule.rs`, decisions in `logic.rs`, the OS in `os.rs`.

mod logic;
mod os;
mod rule;

use trigger_core::{
    Availability, FieldInfo, Handle, KindInfo, Platforms, Rule, RuleError, Sink, Source,
    SourceError,
};

const REPO: FieldInfo = FieldInfo {
    name: "repo",
    doc: "Absolute path of the repository, usually ${repo.root}",
    required: true,
};

const BRANCH: FieldInfo = FieldInfo {
    name: "branch",
    doc: "Glob for the branch name, `*` does not cross /. Default: any branch",
    required: false,
};

const INTERVAL_S: FieldInfo = FieldInfo {
    name: "interval_s",
    doc: "Seconds between checks, at least 30. Default 300",
    required: false,
};

const FETCH: FieldInfo = FieldInfo {
    name: "fetch",
    doc: "Run git fetch before each check. Default false",
    required: false,
};

const OLDER_THAN_MIN: FieldInfo = FieldInfo {
    name: "older_than_min",
    doc: "Minutes the oldest uncommitted change must have waited. Default 60",
    required: false,
};

const FOLDER: FieldInfo = FieldInfo {
    name: "folder",
    doc: "Absolute path of the folder whose new children are watched",
    required: true,
};

const WHEN_REPO: &[FieldInfo] = &[REPO, BRANCH];
const WHEN_BEHIND: &[FieldInfo] = &[REPO, BRANCH, INTERVAL_S, FETCH];
const WHEN_UNCOMMITTED: &[FieldInfo] = &[REPO, BRANCH, INTERVAL_S, OLDER_THAN_MIN];
const WHEN_CLONED: &[FieldInfo] = &[FOLDER, INTERVAL_S];

const fn field(name: &'static str, doc: &'static str) -> FieldInfo {
    FieldInfo {
        name,
        doc,
        required: true,
    }
}

const P_REPO: FieldInfo = field("repo", "Absolute path of the repository");
const P_BRANCH: FieldInfo = field("branch", "Branch name, empty when HEAD is detached");
const P_COMMIT: FieldInfo = field("commit", "Short hash of HEAD");
const P_MESSAGE: FieldInfo = field("message", "First line of the commit message");

const PAYLOAD_COMMIT: &[FieldInfo] = &[P_REPO, P_BRANCH, P_COMMIT, P_MESSAGE];
const PAYLOAD_SWITCHED: &[FieldInfo] = &[
    P_REPO,
    P_BRANCH,
    P_COMMIT,
    field("from", "Branch switched away from"),
    field("to", "Branch switched to"),
];
const PAYLOAD_CONFLICT: &[FieldInfo] = &[
    P_REPO,
    P_BRANCH,
    field("files", "Number of files with conflicts"),
];
const PAYLOAD_BRANCH_COMMIT: &[FieldInfo] = &[P_REPO, P_BRANCH, P_COMMIT];
const PAYLOAD_TAG: &[FieldInfo] = &[P_REPO, P_BRANCH, field("tag", "Name of the new tag")];
const PAYLOAD_STASH: &[FieldInfo] = &[P_REPO, P_BRANCH];
const PAYLOAD_BEHIND: &[FieldInfo] = &[
    P_REPO,
    P_BRANCH,
    field("behind", "Commits the branch is behind its upstream"),
];
const PAYLOAD_UNCOMMITTED: &[FieldInfo] = &[
    P_REPO,
    P_BRANCH,
    field(
        "minutes",
        "Minutes the oldest uncommitted change has waited",
    ),
];
const PAYLOAD_CLONED: &[FieldInfo] = &[field("path", "Absolute path of the new repository")];

/// Every kind this crate fires. The ids are a contract with saved process
/// definitions: add kinds, never rename one.
pub static KINDS: &[KindInfo] = &[
    KindInfo {
        id: "git.commit",
        title: "Commit made",
        summary: "A commit was added to the current branch, or a cherry pick landed.",
        platforms: Platforms::ALL,
        permission: None,
        when: WHEN_REPO,
        payload: PAYLOAD_COMMIT,
    },
    KindInfo {
        id: "git.branch-switched",
        title: "Branch switched",
        summary: "HEAD moved from one branch to another. The branch glob matches the new branch.",
        platforms: Platforms::ALL,
        permission: None,
        when: WHEN_REPO,
        payload: PAYLOAD_SWITCHED,
    },
    KindInfo {
        id: "git.pulled",
        title: "Pulled",
        summary: "The current branch moved by a pull or a merge.",
        platforms: Platforms::ALL,
        permission: None,
        when: WHEN_REPO,
        payload: PAYLOAD_COMMIT,
    },
    KindInfo {
        id: "git.merge-conflict",
        title: "Merge conflict",
        summary: "A merge stopped with conflicting files.",
        platforms: Platforms::ALL,
        permission: None,
        when: WHEN_REPO,
        payload: PAYLOAD_CONFLICT,
    },
    KindInfo {
        id: "git.rebase-stopped",
        title: "Rebase stopped",
        summary: "A rebase stopped for a conflict or an edit.",
        platforms: Platforms::ALL,
        permission: None,
        when: WHEN_REPO,
        payload: PAYLOAD_BRANCH_COMMIT,
    },
    KindInfo {
        id: "git.tag-created",
        title: "Tag created",
        summary: "A tag was created in the repository.",
        platforms: Platforms::ALL,
        permission: None,
        when: WHEN_REPO,
        payload: PAYLOAD_TAG,
    },
    KindInfo {
        id: "git.stash-created",
        title: "Stash created",
        summary: "Work was stashed.",
        platforms: Platforms::ALL,
        permission: None,
        when: WHEN_REPO,
        payload: PAYLOAD_STASH,
    },
    KindInfo {
        id: "git.detached-head",
        title: "HEAD detached",
        summary: "HEAD left a branch and points at a commit. The branch glob matches the branch left.",
        platforms: Platforms::ALL,
        permission: None,
        when: WHEN_REPO,
        payload: PAYLOAD_BRANCH_COMMIT,
    },
    KindInfo {
        id: "git.behind-remote",
        title: "Behind remote",
        summary: "The branch fell behind its upstream. Checked on an interval.",
        platforms: Platforms::ALL,
        permission: None,
        when: WHEN_BEHIND,
        payload: PAYLOAD_BEHIND,
    },
    KindInfo {
        id: "git.uncommitted-older-than",
        title: "Uncommitted work is old",
        summary: "The oldest uncommitted change to a tracked file passed the age limit. Checked on an interval.",
        platforms: Platforms::ALL,
        permission: None,
        when: WHEN_UNCOMMITTED,
        payload: PAYLOAD_UNCOMMITTED,
    },
    KindInfo {
        id: "git.repo-cloned",
        title: "Repository cloned",
        summary: "A new repository appeared directly inside a folder. Checked on an interval.",
        platforms: Platforms::ALL,
        permission: None,
        when: WHEN_CLONED,
        payload: PAYLOAD_CLONED,
    },
];

pub struct GitSource;

impl Source for GitSource {
    fn id(&self) -> &'static str {
        "git"
    }

    fn kinds(&self) -> &'static [KindInfo] {
        KINDS
    }

    fn availability(&self) -> Availability {
        os::availability()
    }

    fn check(&self, rule: &Rule) -> Result<(), RuleError> {
        rule::parse(rule).map(|_| ())
    }

    fn start(&self, rules: Vec<Rule>, sink: Sink) -> Result<Handle, SourceError> {
        for r in &rules {
            rule::parse(r).map_err(|e| SourceError::Os(e.to_string()))?;
        }
        // The watcher and the thread are added by the glue card (slice C).
        let _ = sink;
        Err(SourceError::Unavailable("not implemented yet".into()))
    }
}
