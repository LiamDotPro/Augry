//! Fires on GitHub issues, reviews, CI runs, pull requests and releases for the signed in user. See README.md.
//!
//! Glue only: the kinds this crate fires, the `Source` implementation, and the
//! thread that feeds `gh` answers through the pure logic into the sink. Rules
//! live in `rule.rs`, decisions in `logic.rs`, the OS in `os.rs`.

mod logic;
mod os;
mod rule;

use trigger_core::{
    Availability, FieldInfo, Handle, KindInfo, Platforms, Rule, RuleError, Sink, Source,
    SourceError,
};

const fn req(name: &'static str, doc: &'static str) -> FieldInfo {
    FieldInfo {
        name,
        doc,
        required: true,
    }
}

const fn opt(name: &'static str, doc: &'static str) -> FieldInfo {
    FieldInfo {
        name,
        doc,
        required: false,
    }
}

const REPO: FieldInfo = req(
    "repo",
    "Repository as owner/name, such as cli/cli. Use ${repo.remote} for the repository of the pane's origin",
);
const INTERVAL: FieldInfo = opt(
    "interval_s",
    "Seconds between checks. Default 120, never below 60",
);
const BRANCH_CI: FieldInfo = opt(
    "branch",
    "Glob for the branch the run was on. Default every branch",
);
const BRANCH_BASE: FieldInfo = opt(
    "branch",
    "Glob for the branch the pull request was merged into. Default every branch",
);
const AUTHOR: FieldInfo = opt(
    "author",
    "Logins whose pull requests count, as a list. Default dependabot[bot] and renovate[bot]",
);

const WHEN_REPO: &[FieldInfo] = &[REPO, INTERVAL];
const WHEN_CI: &[FieldInfo] = &[REPO, BRANCH_CI, INTERVAL];
const WHEN_MERGED: &[FieldInfo] = &[REPO, BRANCH_BASE, INTERVAL];
const WHEN_DEPENDENCY: &[FieldInfo] = &[REPO, AUTHOR, INTERVAL];

const PAYLOAD_ISSUE: &[FieldInfo] = &[
    req("number", "Issue number"),
    req("title", "Issue title"),
    req("url", "Web address of the issue"),
    req("author", "Login of whoever opened it"),
    req("assignees", "Logins of everyone assigned, comma separated"),
];

const PAYLOAD_REVIEW: &[FieldInfo] = &[
    req("number", "Pull request number"),
    req("title", "Pull request title"),
    req("url", "Web address of the pull request"),
    req("author", "Login of whoever opened it"),
    req("head", "Branch the changes are on"),
    req("base", "Branch the changes would merge into"),
];

const PAYLOAD_CI: &[FieldInfo] = &[
    req("id", "Workflow run id"),
    req("name", "Workflow name"),
    req("branch", "Branch the run was on"),
    req("sha", "Commit the run was on"),
    req(
        "conclusion",
        "How the run ended, such as failure, timed_out or success",
    ),
    req("url", "Web address of the run"),
    req(
        "event",
        "What started the run, such as push or pull_request",
    ),
    req("actor", "Login of whoever triggered the run"),
    req("updated_at", "When the run finished, ISO 8601"),
];

const PAYLOAD_COMMENT: &[FieldInfo] = &[
    req("id", "Comment id"),
    req("url", "Web address of the comment"),
    req("author", "Login of whoever commented"),
    req("body", "The comment text, cut to 200 characters"),
    req("created_at", "When it was written, ISO 8601"),
];

const PAYLOAD_MERGED: &[FieldInfo] = &[
    req("number", "Pull request number"),
    req("title", "Pull request title"),
    req("url", "Web address of the pull request"),
    req("author", "Login of whoever opened it"),
    req("merged_at", "When it was merged, ISO 8601"),
    req("base", "Branch it was merged into"),
];

const PAYLOAD_RELEASE: &[FieldInfo] = &[
    req("tag", "Tag name of the release"),
    req("name", "Release title"),
    req("prerelease", "true for a pre-release, otherwise false"),
    req("url", "Web address of the release"),
    req("published_at", "When it was published, ISO 8601"),
    req("author", "Login of whoever published it"),
];

const PAYLOAD_DEPENDENCY: &[FieldInfo] = &[
    req("number", "Pull request number"),
    req("title", "Pull request title"),
    req("url", "Web address of the pull request"),
    req("author", "Login of the bot that opened it"),
    req("base", "Branch it targets"),
];

/// Every kind this crate fires. The ids are a contract with saved process
/// definitions: add kinds, never rename one.
pub static KINDS: &[KindInfo] = &[
    KindInfo {
        id: "github.issue-assigned",
        title: "Issue assigned",
        summary: "An open issue was assigned to you.",
        platforms: Platforms::ALL,
        permission: None,
        when: WHEN_REPO,
        payload: PAYLOAD_ISSUE,
    },
    KindInfo {
        id: "github.review-requested",
        title: "Review requested",
        summary: "Someone asked you to review a pull request.",
        platforms: Platforms::ALL,
        permission: None,
        when: WHEN_REPO,
        payload: PAYLOAD_REVIEW,
    },
    KindInfo {
        id: "github.ci-failed",
        title: "CI failed",
        summary: "A workflow run finished with a failure on a matching branch.",
        platforms: Platforms::ALL,
        permission: None,
        when: WHEN_CI,
        payload: PAYLOAD_CI,
    },
    KindInfo {
        id: "github.ci-passed",
        title: "CI passed",
        summary: "A workflow run finished successfully on a matching branch.",
        platforms: Platforms::ALL,
        permission: None,
        when: WHEN_CI,
        payload: PAYLOAD_CI,
    },
    KindInfo {
        id: "github.pr-commented",
        title: "Pull request commented",
        summary: "Someone else commented on a pull request.",
        platforms: Platforms::ALL,
        permission: None,
        when: WHEN_REPO,
        payload: PAYLOAD_COMMENT,
    },
    KindInfo {
        id: "github.pr-merged",
        title: "Pull request merged",
        summary: "A pull request was merged into a matching branch.",
        platforms: Platforms::ALL,
        permission: None,
        when: WHEN_MERGED,
        payload: PAYLOAD_MERGED,
    },
    KindInfo {
        id: "github.release-published",
        title: "Release published",
        summary: "A new release was published.",
        platforms: Platforms::ALL,
        permission: None,
        when: WHEN_REPO,
        payload: PAYLOAD_RELEASE,
    },
    KindInfo {
        id: "github.dependency-pr-opened",
        title: "Dependency update opened",
        summary: "Dependabot or Renovate opened a pull request.",
        platforms: Platforms::ALL,
        permission: None,
        when: WHEN_DEPENDENCY,
        payload: PAYLOAD_DEPENDENCY,
    },
];

pub struct GithubSource;

impl Source for GithubSource {
    fn id(&self) -> &'static str {
        "github"
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
        let _armed = rules
            .iter()
            .map(|r| {
                rule::parse(r).map(|parsed| logic::Armed {
                    id: r.id.clone(),
                    kind: r.kind.clone(),
                    parsed,
                })
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| SourceError::Os(e.to_string()))?;
        // The poll thread is added by slice C.
        let _ = sink;
        Err(SourceError::Unavailable("not implemented yet".into()))
    }
}
