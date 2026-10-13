//! The `when` object of a rule, typed, with the branch glob compiled. Pure.

use std::path::PathBuf;

use globset::{GlobBuilder, GlobMatcher};
use serde::Deserialize;
use trigger_core::{Rule, RuleError};

/// The polled kinds check at most this often.
pub const MIN_INTERVAL_S: u64 = 30;

fn default_interval_s() -> u64 {
    300
}

fn default_older_than_min() -> u64 {
    60
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct When {
    #[serde(default)]
    pub repo: Option<PathBuf>,
    #[serde(default)]
    pub folder: Option<PathBuf>,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default = "default_interval_s")]
    pub interval_s: u64,
    #[serde(default = "default_older_than_min")]
    pub older_than_min: u64,
    #[serde(default)]
    pub fetch: bool,
}

/// A parsed rule: the typed fields plus the compiled branch glob.
#[derive(Debug, Clone)]
pub struct Parsed {
    pub when: When,
    pub branch: Option<GlobMatcher>,
}

impl Parsed {
    /// True when the rule has no branch glob, or the glob matches `name`.
    /// A rule with a glob never matches when there is no branch name.
    pub fn branch_matches(&self, name: Option<&str>) -> bool {
        match (&self.branch, name) {
            (None, _) => true,
            (Some(glob), Some(name)) => glob.is_match(name),
            (Some(_), None) => false,
        }
    }
}

fn field_error(field: &str, problem: &str) -> RuleError {
    RuleError::Field {
        field: field.into(),
        problem: problem.into(),
    }
}

/// Checks that the kind belongs to this crate and the fields parse. Used by
/// `check` (the editor) and by `start`.
pub fn parse(rule: &Rule) -> Result<Parsed, RuleError> {
    if !crate::KINDS.iter().any(|k| k.id == rule.kind) {
        return Err(RuleError::UnknownKind(rule.kind.clone()));
    }
    let value = serde_json::Value::Object(rule.when.clone().into_iter().collect());
    let when: When = serde_json::from_value(value).map_err(|e| RuleError::Field {
        field: "when".into(),
        problem: e.to_string(),
    })?;

    if rule.kind == "git.repo-cloned" {
        if when.repo.is_some() {
            return Err(field_error(
                "repo",
                "is not used by git.repo-cloned, use folder",
            ));
        }
        match &when.folder {
            None => return Err(field_error("folder", "is required")),
            Some(folder) if !folder.is_absolute() => {
                return Err(field_error("folder", "must be an absolute path"))
            }
            Some(_) => {}
        }
    } else {
        match &when.repo {
            None => return Err(field_error("repo", "is required")),
            Some(repo) if !repo.is_absolute() => {
                return Err(field_error("repo", "must be an absolute path"))
            }
            Some(_) => {}
        }
    }

    if when.interval_s < MIN_INTERVAL_S {
        return Err(field_error("interval_s", "must be at least 30"));
    }

    // `*` does not cross `/`, so `release/*` matches `release/1.0` but not
    // `release/1/2`. Branch names are case sensitive, so the glob is too.
    let branch = match &when.branch {
        None => None,
        Some(pattern) => {
            let glob = GlobBuilder::new(pattern)
                .literal_separator(true)
                .build()
                .map_err(|e| field_error("branch", &e.to_string()))?;
            Some(glob.compile_matcher())
        }
    };
    Ok(Parsed { when, branch })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use trigger_core::testkit::rule;

    fn dir() -> String {
        std::env::temp_dir().to_string_lossy().into_owned()
    }

    fn parsed(kind: &str, when: serde_json::Value) -> Result<Parsed, RuleError> {
        parse(&rule("p/t", kind, when))
    }

    fn field_of(err: RuleError) -> String {
        match err {
            RuleError::Field { field, .. } => field,
            other => panic!("expected a field error, got {other:?}"),
        }
    }

    #[test]
    fn defaults_are_filled_in() {
        let p = parsed("git.commit", json!({ "repo": dir() })).unwrap();
        assert_eq!(p.when.repo, Some(PathBuf::from(dir())));
        assert_eq!(p.when.folder, None);
        assert_eq!(p.when.branch, None);
        assert_eq!(p.when.interval_s, 300);
        assert_eq!(p.when.older_than_min, 60);
        assert!(!p.when.fetch);
        assert!(p.branch.is_none());
    }

    #[test]
    fn all_fields_are_read() {
        let p = parsed(
            "git.behind-remote",
            json!({ "repo": dir(), "branch": "main", "interval_s": 60, "older_than_min": 10, "fetch": true }),
        )
        .unwrap();
        assert_eq!(p.when.branch.as_deref(), Some("main"));
        assert_eq!(p.when.interval_s, 60);
        assert_eq!(p.when.older_than_min, 10);
        assert!(p.when.fetch);
    }

    #[test]
    fn rejects_unknown_fields_and_kinds() {
        let typo = parsed("git.commit", json!({ "repo": dir(), "brnach": "x" }));
        assert_eq!(field_of(typo.unwrap_err()), "when");
        let other = parsed("fs.file-saved", json!({ "repo": dir() }));
        assert!(matches!(other, Err(RuleError::UnknownKind(_))));
    }

    #[test]
    fn every_kind_but_repo_cloned_requires_an_absolute_repo() {
        for kind in crate::KINDS
            .iter()
            .map(|k| k.id)
            .filter(|id| *id != "git.repo-cloned")
        {
            let missing = parsed(kind, json!({}));
            assert_eq!(field_of(missing.unwrap_err()), "repo", "{kind}");
            let relative = parsed(kind, json!({ "repo": "work/app" }));
            assert_eq!(field_of(relative.unwrap_err()), "repo", "{kind}");
            assert!(parsed(kind, json!({ "repo": dir() })).is_ok(), "{kind}");
        }
    }

    #[test]
    fn repo_cloned_requires_an_absolute_folder_and_rejects_repo() {
        let ok = parsed("git.repo-cloned", json!({ "folder": dir() })).unwrap();
        assert_eq!(ok.when.folder, Some(PathBuf::from(dir())));
        let missing = parsed("git.repo-cloned", json!({}));
        assert_eq!(field_of(missing.unwrap_err()), "folder");
        let relative = parsed("git.repo-cloned", json!({ "folder": "projects" }));
        assert_eq!(field_of(relative.unwrap_err()), "folder");
        let repo = parsed("git.repo-cloned", json!({ "folder": dir(), "repo": dir() }));
        assert_eq!(field_of(repo.unwrap_err()), "repo");
    }

    #[test]
    fn interval_must_be_at_least_thirty_seconds() {
        let short = parsed(
            "git.behind-remote",
            json!({ "repo": dir(), "interval_s": 29 }),
        );
        assert_eq!(field_of(short.unwrap_err()), "interval_s");
        let edge = parsed(
            "git.behind-remote",
            json!({ "repo": dir(), "interval_s": 30 }),
        );
        assert!(edge.is_ok());
        let cloned = parsed(
            "git.repo-cloned",
            json!({ "folder": dir(), "interval_s": 5 }),
        );
        assert_eq!(field_of(cloned.unwrap_err()), "interval_s");
    }

    #[test]
    fn a_bad_branch_glob_names_its_field() {
        let err = parsed("git.commit", json!({ "repo": dir(), "branch": "[" })).unwrap_err();
        assert_eq!(field_of(err), "branch");
    }

    #[test]
    fn branch_glob_matching() {
        let none = parsed("git.commit", json!({ "repo": dir() })).unwrap();
        assert!(none.branch_matches(Some("anything")));
        assert!(none.branch_matches(None));

        let release = parsed(
            "git.commit",
            json!({ "repo": dir(), "branch": "release/*" }),
        )
        .unwrap();
        assert!(release.branch_matches(Some("release/1.0")));
        assert!(!release.branch_matches(Some("release/1/2")));
        assert!(!release.branch_matches(Some("main")));
        assert!(!release.branch_matches(None));

        let deep = parsed(
            "git.commit",
            json!({ "repo": dir(), "branch": "feature/**" }),
        )
        .unwrap();
        assert!(deep.branch_matches(Some("feature/a/b")));

        let exact = parsed("git.commit", json!({ "repo": dir(), "branch": "main" })).unwrap();
        assert!(exact.branch_matches(Some("main")));
        assert!(!exact.branch_matches(Some("Main")));
    }
}
