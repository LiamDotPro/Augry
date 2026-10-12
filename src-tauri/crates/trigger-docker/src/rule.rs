//! The `when` object of a rule, typed, with its globs compiled. Pure.

// Removed when lib.rs reads everything in this file.
#![allow(dead_code)]

use globset::{GlobBuilder, GlobMatcher};
use serde::Deserialize;
use trigger_core::{Rule, RuleError};

/// Which engine a rule listens to. `Auto` means whichever answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EngineChoice {
    Docker,
    Podman,
    #[default]
    Auto,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct When {
    pub name: Option<String>,
    pub image: Option<String>,
    pub label: Option<String>,
    #[serde(default)]
    pub engine: EngineChoice,
}

/// A parsed rule: the typed fields plus the compiled matchers.
#[derive(Debug, Clone)]
pub struct Parsed {
    pub when: When,
    pub name: Option<GlobMatcher>,
    pub image: Option<GlobMatcher>,
    /// The label split at the first `=`.
    pub label: Option<(String, String)>,
}

/// `*` crosses `/` because image references have slashes. Case sensitive:
/// container names and image references are.
fn build_glob(pattern: &str) -> Result<GlobMatcher, String> {
    let glob = GlobBuilder::new(pattern)
        .literal_separator(false)
        .build()
        .map_err(|e| format!("{pattern}: {e}"))?;
    Ok(glob.compile_matcher())
}

fn field_error(field: &str, problem: impl Into<String>) -> RuleError {
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
    let when: When =
        serde_json::from_value(value).map_err(|e| field_error("when", e.to_string()))?;
    let name = match &when.name {
        Some(p) => Some(build_glob(p).map_err(|e| field_error("name", e))?),
        None => None,
    };
    let image = match &when.image {
        Some(p) => Some(build_glob(p).map_err(|e| field_error("image", e))?),
        None => None,
    };
    let label = match &when.label {
        Some(text) => match text.split_once('=') {
            Some((key, value)) if !key.trim().is_empty() => {
                Some((key.trim().to_string(), value.trim().to_string()))
            }
            _ => return Err(field_error("label", "must be key=value")),
        },
        None => None,
    };
    Ok(Parsed {
        when,
        name,
        image,
        label,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use trigger_core::testkit::rule;

    fn parsed(when: serde_json::Value) -> Result<Parsed, RuleError> {
        parse(&rule("p/t", "docker.container-started", when))
    }

    fn field_of(err: RuleError) -> String {
        match err {
            RuleError::Field { field, .. } => field,
            other => panic!("expected a field error, got {other:?}"),
        }
    }

    #[test]
    fn an_empty_when_is_valid_and_defaults_to_auto() {
        let p = parsed(json!({})).unwrap();
        assert_eq!(p.when.engine, EngineChoice::Auto);
        assert!(p.name.is_none() && p.image.is_none() && p.label.is_none());
    }

    #[test]
    fn all_fields_are_read() {
        let p = parsed(
            json!({ "name": "web-*", "image": "nginx*", "label": "com.example.team=x", "engine": "podman" }),
        )
        .unwrap();
        assert_eq!(p.when.engine, EngineChoice::Podman);
        assert!(p.name.unwrap().is_match("web-1"));
        assert!(p.image.unwrap().is_match("nginx:1.27"));
        assert_eq!(
            p.label,
            Some(("com.example.team".to_string(), "x".to_string()))
        );
    }

    #[test]
    fn the_engine_is_one_of_three_words() {
        for ok in ["docker", "podman", "auto"] {
            assert!(parsed(json!({ "engine": ok })).is_ok(), "{ok}");
        }
        let bad = parsed(json!({ "engine": "lxc" })).unwrap_err();
        assert_eq!(field_of(bad), "when");
    }

    #[test]
    fn a_label_must_be_key_equals_value() {
        assert!(parsed(json!({ "label": "a=" })).is_ok());
        for bad in ["team", "=x", " =x", ""] {
            let err = parsed(json!({ "label": bad })).unwrap_err();
            assert_eq!(field_of(err), "label", "{bad}");
        }
    }

    #[test]
    fn a_label_value_may_contain_equals() {
        let p = parsed(json!({ "label": "a=b=c" })).unwrap();
        assert_eq!(p.label, Some(("a".to_string(), "b=c".to_string())));
    }

    #[test]
    fn a_bad_glob_names_its_field() {
        let name = parsed(json!({ "name": "[" })).unwrap_err();
        assert_eq!(field_of(name), "name");
        let image = parsed(json!({ "image": "{a" })).unwrap_err();
        assert_eq!(field_of(image), "image");
    }

    #[test]
    fn globs_cross_slashes_and_are_case_sensitive() {
        let p = parsed(json!({ "image": "myorg/*" })).unwrap();
        let m = p.image.unwrap();
        assert!(m.is_match("myorg/app/api:1"));
        assert!(!m.is_match("MyOrg/app"));
    }

    #[test]
    fn rejects_unknown_fields_and_kinds() {
        let typo = parsed(json!({ "imge": "x" })).unwrap_err();
        assert_eq!(field_of(typo), "when");
        let other = parse(&rule("p/t", "fs.file-saved", json!({})));
        assert!(matches!(other, Err(RuleError::UnknownKind(_))));
    }
}
