//! The `when` object of a rule, typed, with its globs compiled. Pure.

use std::path::PathBuf;

use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use serde::Deserialize;
use trigger_core::{Rule, RuleError};

/// Always ignored, before the user's own `ignore` globs. Directory names come
/// in two forms so an event on the directory itself is ignored too.
pub const DEFAULT_IGNORES: &[&str] = &[
    "**/.git",
    "**/.git/**",
    "**/node_modules",
    "**/node_modules/**",
    "**/target",
    "**/target/**",
    "**/.DS_Store",
    "**/*.crdownload",
    "**/*.part",
    "**/*.download",
    "**/*.tmp",
    "**/*~",
    "**/*.swp",
    "**/*.swx",
];

fn default_glob() -> String {
    "**/*".to_string()
}

fn default_settle_ms() -> u64 {
    500
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct When {
    pub path: PathBuf,
    #[serde(default = "default_glob")]
    pub glob: String,
    #[serde(default)]
    pub ignore: Vec<String>,
    #[serde(default = "default_settle_ms")]
    pub settle_ms: u64,
}

/// A parsed rule: the typed fields plus the two compiled matchers. Both
/// matchers are applied to the path relative to `when.path`.
#[derive(Debug, Clone)]
pub struct Parsed {
    pub when: When,
    pub glob: GlobSet,
    pub ignore: GlobSet,
}

/// Case insensitive, `*` does not cross `/`. macOS and Windows file systems
/// are case insensitive, so `.PART` must match `*.part`.
fn build_set<'a>(patterns: impl Iterator<Item = &'a str>) -> Result<GlobSet, String> {
    let mut set = GlobSetBuilder::new();
    for pattern in patterns {
        let glob = GlobBuilder::new(pattern)
            .literal_separator(true)
            .case_insensitive(true)
            .build()
            .map_err(|e| format!("{pattern}: {e}"))?;
        set.add(glob);
    }
    set.build().map_err(|e| e.to_string())
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
    if !when.path.is_absolute() {
        return Err(RuleError::Field {
            field: "path".into(),
            problem: "must be an absolute path".into(),
        });
    }
    let glob =
        build_set(std::iter::once(when.glob.as_str())).map_err(|problem| RuleError::Field {
            field: "glob".into(),
            problem,
        })?;
    let patterns = DEFAULT_IGNORES
        .iter()
        .copied()
        .chain(when.ignore.iter().map(String::as_str));
    let ignore = build_set(patterns).map_err(|problem| RuleError::Field {
        field: "ignore".into(),
        problem,
    })?;
    Ok(Parsed { when, glob, ignore })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::path::Path;
    use trigger_core::testkit::rule;

    fn dir() -> String {
        std::env::temp_dir().to_string_lossy().into_owned()
    }

    fn parsed(when: serde_json::Value) -> Result<Parsed, RuleError> {
        parse(&rule("p/t", "fs.file-saved", when))
    }

    fn field_of(err: RuleError) -> String {
        match err {
            RuleError::Field { field, .. } => field,
            other => panic!("expected a field error, got {other:?}"),
        }
    }

    #[test]
    fn defaults_are_filled_in() {
        let p = parsed(json!({ "path": dir() })).unwrap();
        assert_eq!(p.when.glob, "**/*");
        assert!(p.when.ignore.is_empty());
        assert_eq!(p.when.settle_ms, 500);
    }

    #[test]
    fn all_fields_are_read() {
        let p = parsed(
            json!({ "path": dir(), "glob": "**/*.md", "ignore": ["drafts/**"], "settle_ms": 50 }),
        )
        .unwrap();
        assert_eq!(p.when.glob, "**/*.md");
        assert_eq!(p.when.ignore, vec!["drafts/**".to_string()]);
        assert_eq!(p.when.settle_ms, 50);
    }

    #[test]
    fn rejects_unknown_fields_kinds_and_missing_path() {
        let typo = parsed(json!({ "path": dir(), "globs": "x" }));
        assert_eq!(field_of(typo.unwrap_err()), "when");
        let missing = parsed(json!({ "glob": "**/*.md" }));
        assert_eq!(field_of(missing.unwrap_err()), "when");
        let other = parse(&rule("p/t", "git.commit", json!({ "path": dir() })));
        assert!(matches!(other, Err(RuleError::UnknownKind(_))));
    }

    #[test]
    fn rejects_a_relative_path() {
        let err = parsed(json!({ "path": "docs/notes" })).unwrap_err();
        assert_eq!(field_of(err), "path");
    }

    #[test]
    fn a_bad_glob_or_ignore_names_its_field() {
        let glob = parsed(json!({ "path": dir(), "glob": "[" })).unwrap_err();
        assert_eq!(field_of(glob), "glob");
        let ignore = parsed(json!({ "path": dir(), "ignore": ["{a"] })).unwrap_err();
        assert_eq!(field_of(ignore), "ignore");
    }

    #[test]
    fn glob_matches_relative_paths_without_crossing_separators() {
        let p = parsed(json!({ "path": dir(), "glob": "*.md" })).unwrap();
        assert!(p.glob.is_match(Path::new("a.md")));
        assert!(p.glob.is_match(Path::new("A.MD")));
        assert!(!p.glob.is_match(Path::new("docs/a.md")));
        let deep = parsed(json!({ "path": dir(), "glob": "**/*.md" })).unwrap();
        assert!(deep.glob.is_match(Path::new("docs/a.md")));
        assert!(deep.glob.is_match(Path::new("top.md")));
        assert!(!deep.glob.is_match(Path::new("docs/a.txt")));
    }

    #[test]
    fn default_ignores_cover_the_usual_noise() {
        let p = parsed(json!({ "path": dir() })).unwrap();
        for ignored in [
            ".git",
            "a/.git/HEAD",
            "node_modules/x/y.js",
            "src/target/x",
            ".DS_Store",
            "a/.DS_Store",
            "x.PART",
            "f.crdownload",
            "a/f.tmp",
            "notes.md~",
            "a/.f.swp",
        ] {
            assert!(
                p.ignore.is_match(Path::new(ignored)),
                "{ignored} should be ignored"
            );
        }
        for kept in ["docs/a.md", "a/targetx/f", ".gitignore", "a.txt"] {
            assert!(
                !p.ignore.is_match(Path::new(kept)),
                "{kept} should not be ignored"
            );
        }
    }

    #[test]
    fn user_ignores_are_added_to_the_defaults() {
        let p = parsed(json!({ "path": dir(), "ignore": ["drafts/**"] })).unwrap();
        assert!(p.ignore.is_match(Path::new("drafts/a.md")));
        assert!(p.ignore.is_match(Path::new("x.tmp")));
    }

    #[cfg(windows)]
    #[test]
    fn windows_separators_are_normalised_by_globset() {
        let p = parsed(json!({ "path": dir(), "glob": "**/*.md" })).unwrap();
        assert!(p.glob.is_match(Path::new(r"docs\a.md")));
    }
}
