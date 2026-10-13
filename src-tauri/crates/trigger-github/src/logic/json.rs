//! Reading GitHub's JSON without a struct per endpoint. Real list items carry
//! dozens of fields we never use, and some of the ones we do use are `null`
//! (a release without a name, a run without a branch), so every reader here
//! gives an empty value for a missing or `null` field instead of failing.

use std::fmt;

pub use serde_json::Value;

/// The body was not the JSON we expected. Carries a short message, never the body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BodyError(pub String);

impl fmt::Display for BodyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for BodyError {}

/// The items of a list response. With `wrapper` the list is inside an object
/// under that key (`workflow_runs` for `actions/runs`), otherwise the body is
/// the array itself.
pub fn list(body: &str, wrapper: Option<&str>) -> Result<Vec<Value>, BodyError> {
    let value: Value = serde_json::from_str(body)
        .map_err(|e| BodyError(format!("the answer was not JSON: {e}")))?;
    let array = match wrapper {
        Some(key) => value.get(key),
        None => Some(&value),
    };
    match array.and_then(Value::as_array) {
        Some(items) => Ok(items.clone()),
        None => Err(BodyError("the answer was not a list".into())),
    }
}

/// A string at a JSON pointer such as `/user/login`. Empty when missing or not text.
pub fn text<'a>(v: &'a Value, pointer: &str) -> &'a str {
    v.pointer(pointer).and_then(Value::as_str).unwrap_or("")
}

/// A number at a JSON pointer. 0 when missing or not a whole number.
pub fn num(v: &Value, pointer: &str) -> u64 {
    v.pointer(pointer).and_then(Value::as_u64).unwrap_or(0)
}

/// A boolean at a JSON pointer. False when missing or not a boolean.
pub fn flag(v: &Value, pointer: &str) -> bool {
    v.pointer(pointer).and_then(Value::as_bool).unwrap_or(false)
}

/// The item's `id` as text, for a seen set. `None` when it has no usable id.
pub fn id_key(v: &Value) -> Option<String> {
    match num(v, "/id") {
        0 => None,
        id => Some(id.to_string()),
    }
}

/// True when the pointer finds something that is not `null`.
pub fn has(v: &Value, pointer: &str) -> bool {
    v.pointer(pointer).is_some_and(|x| !x.is_null())
}

/// The `login` of each object in an array at the pointer, such as `/assignees`.
pub fn logins<'a>(v: &'a Value, pointer: &str) -> Vec<&'a str> {
    v.pointer(pointer)
        .and_then(Value::as_array)
        .map(|list| list.iter().map(|u| text(u, "/login")).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn list_reads_an_array_or_a_wrapped_array() {
        assert_eq!(list("[{\"id\":1},{\"id\":2}]", None).unwrap().len(), 2);
        let wrapped = "{\"total_count\":1,\"workflow_runs\":[{\"id\":1}]}";
        assert_eq!(list(wrapped, Some("workflow_runs")).unwrap().len(), 1);
        assert_eq!(list("[]", None).unwrap().len(), 0);
    }

    #[test]
    fn list_refuses_anything_else_without_echoing_it() {
        for (body, wrapper) in [
            ("not json", None),
            ("{\"message\":\"Not Found\"}", None),
            ("{\"a\":1}", Some("workflow_runs")),
            ("", None),
        ] {
            let err = list(body, wrapper).unwrap_err();
            assert!(!err.to_string().contains("Not Found"), "{err}");
        }
    }

    #[test]
    fn readers_give_empty_values_for_missing_and_null() {
        let v = json!({
            "id": 7, "name": null, "draft": true,
            "user": { "login": "octocat" },
            "assignees": [{ "login": "a" }, { "login": "b" }]
        });
        assert_eq!(num(&v, "/id"), 7);
        assert_eq!(num(&v, "/missing"), 0);
        assert_eq!(text(&v, "/user/login"), "octocat");
        assert_eq!(text(&v, "/name"), "");
        assert_eq!(text(&v, "/nope/deeper"), "");
        assert!(flag(&v, "/draft"));
        assert!(!flag(&v, "/other"));
        assert!(has(&v, "/user"));
        assert!(!has(&v, "/name"));
        assert!(!has(&v, "/missing"));
        assert_eq!(logins(&v, "/assignees"), ["a", "b"]);
        assert_eq!(id_key(&v), Some("7".to_string()));
        assert_eq!(id_key(&json!({ "id": null })), None);
        assert_eq!(id_key(&json!({})), None);
        assert!(logins(&v, "/user").is_empty());
    }
}
