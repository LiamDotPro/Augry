---
id: T30
epic: 001-trigger-core
title: Add TriggersConfig and the local definition store
status: todo
depends_on: [T08]
size: M
model: haiku
files:
  - src-tauri/src/state/mod.rs
  - src-tauri/src/triggers/mod.rs
  - src-tauri/src/triggers/store.rs
  - src-tauri/src/lib.rs
---

## Goal

`Config` has a `triggers` block (`enabled`, `armed`) that is saved in `config.json`, and `src-tauri/src/triggers/store.rs` reads the process definition files from a folder and reports which ones cannot be used. Nothing uses either yet.

## Read first

- `plans/001-trigger-core/epic.md` Decisions 10 and 11 (what a definition file is, and the config block)
- `src-tauri/src/state/mod.rs:15-130` `Config`, its `Default`, `ConfigPatch` and `Config::apply`
- `src-tauri/src/lib.rs:1-20` the module list

## Steps

1. Before changing anything, run `cd src-tauri && cargo clippy -p terminal-grid --all-targets 2>&1 | grep -c warning` and write the number in the Log. It is the baseline for the clippy check in Acceptance.

2. In `src-tauri/src/state/mod.rs`, add the field `triggers` to `Config`. Find these lines (the only place that has `Option<String>,` followed by a closing brace):

```rust
    pub finished_color: Option<String>,
}
```

Replace them with:

```rust
    pub finished_color: Option<String>,
    /// Process triggers: the master switch and which definitions are armed.
    pub triggers: TriggersConfig,
}
```

Then add these two types directly above the line that starts `const THEMES`, with a blank line before and after:

```rust
/// Process triggers, kept in `config.json` and never in the session or the
/// cloud (plans/001-trigger-core/epic.md Decision 11).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TriggersConfig {
    pub enabled: bool,
    /// Ids of the local process definitions the user has armed.
    pub armed: Vec<String>,
}

/// A partial update of `TriggersConfig`; a missing field is left alone.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TriggersPatch {
    pub enabled: Option<bool>,
    pub armed: Option<Vec<String>>,
}
```

3. In the same file, in `impl Default for Config`, find `            finished_color: None,` (the last field line) and add a line after it:

```rust
            triggers: TriggersConfig::default(),
```

4. In the same file, in `ConfigPatch`, find the last field `    pub finished_color: Option<Option<String>>,` and add a line after it:

```rust
    pub triggers: Option<TriggersPatch>,
```

5. In the same file, at the end of `Config::apply` (after the `match patch.finished_color { ... }` block, before the closing `}` of the method), add:

```rust
        if let Some(triggers) = patch.triggers {
            if let Some(enabled) = triggers.enabled {
                self.triggers.enabled = enabled;
            }
            if let Some(armed) = triggers.armed {
                let mut seen = Vec::new();
                for id in armed {
                    if !id.is_empty() && !seen.contains(&id) {
                        seen.push(id);
                    }
                }
                self.triggers.armed = seen;
            }
        }
```

6. In the same file, inside `mod tests`, directly above its final closing `}` (the last line of the file), add these four tests:

```rust

    #[test]
    fn triggers_are_off_and_empty_by_default() {
        let config = Config::default();
        assert!(!config.triggers.enabled);
        assert!(config.triggers.armed.is_empty());
        let json = serde_json::to_value(&config).expect("config serialises");
        assert_eq!(json["triggers"]["enabled"], false);
        assert_eq!(json["triggers"]["armed"], serde_json::json!([]));
    }

    #[test]
    fn a_config_written_before_triggers_loads_with_them_off() {
        let config: Config =
            serde_json::from_str(r#"{"version":1,"fontSize":18}"#).expect("older config parses");
        assert_eq!(config.triggers, TriggersConfig::default());
    }

    #[test]
    fn a_triggers_patch_changes_only_the_fields_it_carries() {
        let mut config = Config::default();
        let patch: ConfigPatch =
            serde_json::from_str(r#"{"triggers":{"armed":["a","b"]}}"#).expect("patch parses");
        config.apply(patch);
        assert!(!config.triggers.enabled);
        assert_eq!(
            config.triggers.armed,
            vec!["a".to_string(), "b".to_string()]
        );

        let patch: ConfigPatch =
            serde_json::from_str(r#"{"triggers":{"enabled":true}}"#).expect("patch parses");
        config.apply(patch);
        assert!(config.triggers.enabled);
        assert_eq!(config.triggers.armed.len(), 2);
    }

    #[test]
    fn armed_ids_are_deduplicated_and_empty_ones_dropped() {
        let mut config = Config::default();
        config.apply(ConfigPatch {
            triggers: Some(TriggersPatch {
                enabled: None,
                armed: Some(vec!["a".into(), "".into(), "a".into(), "b".into()]),
            }),
            ..Default::default()
        });
        assert_eq!(
            config.triggers.armed,
            vec!["a".to_string(), "b".to_string()]
        );
    }
```

7. In `src-tauri/src/lib.rs`, add the module. Find these two lines:

```rust
#[cfg(target_os = "macos")]
mod traffic_lights;
```

and add a line right after them:

```rust
pub mod triggers;
```

8. Create `src-tauri/src/triggers/mod.rs` with exactly:

```rust
//! Process triggers in the app: the local process definitions, the rules
//! worked out from them, and the glue to `trigger-hub`. Design:
//! plans/001-trigger-core/epic.md Decisions 10 to 16. Later cards add the
//! rest of the module; each adds its own `pub mod` line.

pub mod store;
```

9. Create `src-tauri/src/triggers/store.rs` with exactly this content. It reads `*.json` files, sorted by id; a file that cannot be used is still listed with `problem` set. The tests are in the same file.

```rust
//! Local process definitions: one JSON file per process in
//! `<app config dir>/processes/`, the file stem being the process id. Only
//! the fields the triggers need are read; the rest of the file is left alone.
//! Design: plans/001-trigger-core/epic.md Decision 10.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use serde_json::Value;

/// A definition file larger than this is not read.
const MAX_BYTES: u64 = 1_000_000;

#[derive(Debug, Clone, PartialEq)]
pub struct DefinitionTrigger {
    pub id: String,
    pub kind: String,
    pub when: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LocalDefinition {
    /// The file name without `.json`.
    pub id: String,
    /// The `name` field, or the id when the file has none.
    pub name: String,
    pub triggers: Vec<DefinitionTrigger>,
    /// Why the file cannot be used. When set, `triggers` is empty.
    pub problem: Option<String>,
}

impl LocalDefinition {
    fn broken(id: &str, name: Option<String>, problem: String) -> LocalDefinition {
        LocalDefinition {
            id: id.to_string(),
            name: name.unwrap_or_else(|| id.to_string()),
            triggers: Vec::new(),
            problem: Some(problem),
        }
    }
}

/// Reads every `*.json` file in `dir`, sorted by id. The folder is created
/// when it does not exist. A file that cannot be used is still listed, with
/// its `problem` set.
pub fn load(dir: &Path) -> Vec<LocalDefinition> {
    if fs::create_dir_all(dir).is_err() {
        return Vec::new();
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") || !path.is_file() {
            continue;
        }
        let Some(id) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        if id.is_empty() || id.starts_with('.') {
            continue;
        }
        let too_big = entry
            .metadata()
            .map(|m| m.len() > MAX_BYTES)
            .unwrap_or(false);
        if too_big {
            found.push(LocalDefinition::broken(
                id,
                None,
                "the file is larger than 1 MB".into(),
            ));
            continue;
        }
        match fs::read_to_string(&path) {
            Ok(raw) => found.push(parse(id, &raw)),
            Err(error) => found.push(LocalDefinition::broken(
                id,
                None,
                format!("cannot read the file ({:?})", error.kind()),
            )),
        }
    }
    found.sort_by(|a, b| a.id.cmp(&b.id));
    found
}

/// Turns the text of one definition file into a `LocalDefinition`.
pub fn parse(id: &str, raw: &str) -> LocalDefinition {
    // Files edited by hand on Windows often start with a byte order mark.
    let raw = raw.trim_start_matches('\u{feff}');
    let value: Value = match serde_json::from_str(raw) {
        Ok(value) => value,
        Err(error) => {
            return LocalDefinition::broken(id, None, format!("not valid JSON: {error}"));
        }
    };
    let Some(object) = value.as_object() else {
        return LocalDefinition::broken(id, None, "the file is not a JSON object".into());
    };
    let name = object
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .map(str::to_string);
    if object.get("schemaVersion").and_then(Value::as_i64) != Some(1) {
        return LocalDefinition::broken(id, name, "schemaVersion must be 1".into());
    }
    let Some(name) = name else {
        return LocalDefinition::broken(id, None, "name is missing".into());
    };
    match parse_triggers(object.get("triggers")) {
        Ok(triggers) => LocalDefinition {
            id: id.to_string(),
            name,
            triggers,
            problem: None,
        },
        Err(problem) => LocalDefinition::broken(id, Some(name), problem),
    }
}

fn parse_triggers(value: Option<&Value>) -> Result<Vec<DefinitionTrigger>, String> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let Some(list) = value.as_array() else {
        return Err("triggers must be an array".into());
    };
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for (index, item) in list.iter().enumerate() {
        let n = index + 1;
        let Some(object) = item.as_object() else {
            return Err(format!("trigger {n} is not an object"));
        };
        let text = |key: &str| {
            object
                .get(key)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
        };
        let Some(trigger_id) = text("id") else {
            return Err(format!("trigger {n} has no id"));
        };
        let Some(kind) = text("kind") else {
            return Err(format!("trigger {n} has no kind"));
        };
        let when = match object.get("when") {
            None => BTreeMap::new(),
            Some(Value::Object(map)) => map.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
            Some(_) => return Err(format!("trigger {n}: when must be an object")),
        };
        if !seen.insert(trigger_id.to_string()) {
            return Err(format!("trigger id {trigger_id} is used twice"));
        }
        out.push(DefinitionTrigger {
            id: trigger_id.to_string(),
            kind: kind.to_string(),
            when,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = r#"{
        "schemaVersion": 1,
        "name": "Keep the docs tidy",
        "steps": [{"anything": true}],
        "triggers": [
            {"id": "docs-saved", "kind": "fs.file-saved", "when": {"path": "${repo.root}/docs"}},
            {"id": "other", "kind": "fs.file-created"}
        ]
    }"#;

    #[test]
    fn a_good_file_is_read() {
        let def = parse("tidy", GOOD);
        assert_eq!(def.problem, None);
        assert_eq!(def.name, "Keep the docs tidy");
        assert_eq!(def.triggers.len(), 2);
        assert_eq!(def.triggers[0].id, "docs-saved");
        assert_eq!(def.triggers[0].kind, "fs.file-saved");
        assert_eq!(
            def.triggers[0].when["path"],
            Value::String("${repo.root}/docs".into())
        );
        assert!(def.triggers[1].when.is_empty());
    }

    #[test]
    fn a_file_without_triggers_is_fine() {
        let def = parse("a", r#"{"schemaVersion":1,"name":"A"}"#);
        assert_eq!(def.problem, None);
        assert!(def.triggers.is_empty());
    }

    #[test]
    fn a_byte_order_mark_is_ignored() {
        let def = parse("a", "\u{feff}{\"schemaVersion\":1,\"name\":\"A\"}");
        assert_eq!(def.problem, None);
    }

    #[test]
    fn broken_files_name_the_problem() {
        let cases = [
            ("{", "not valid JSON"),
            ("[]", "not a JSON object"),
            (
                r#"{"schemaVersion":2,"name":"A"}"#,
                "schemaVersion must be 1",
            ),
            (r#"{"name":"A"}"#, "schemaVersion must be 1"),
            (r#"{"schemaVersion":1}"#, "name is missing"),
            (r#"{"schemaVersion":1,"name":"  "}"#, "name is missing"),
            (
                r#"{"schemaVersion":1,"name":"A","triggers":{}}"#,
                "triggers must be an array",
            ),
            (
                r#"{"schemaVersion":1,"name":"A","triggers":[1]}"#,
                "trigger 1 is not an object",
            ),
            (
                r#"{"schemaVersion":1,"name":"A","triggers":[{"kind":"k"}]}"#,
                "trigger 1 has no id",
            ),
            (
                r#"{"schemaVersion":1,"name":"A","triggers":[{"id":"x"}]}"#,
                "trigger 1 has no kind",
            ),
            (
                r#"{"schemaVersion":1,"name":"A","triggers":[{"id":"x","kind":"k","when":3}]}"#,
                "when must be an object",
            ),
            (
                r#"{"schemaVersion":1,"name":"A","triggers":[{"id":"x","kind":"k"},{"id":"x","kind":"k"}]}"#,
                "used twice",
            ),
        ];
        for (raw, expected) in cases {
            let def = parse("p", raw);
            let problem = def.problem.unwrap_or_default();
            assert!(problem.contains(expected), "{raw} gave {problem}");
            assert!(def.triggers.is_empty());
        }
    }

    #[test]
    fn a_broken_file_keeps_its_name_when_it_has_one() {
        let def = parse("p", r#"{"schemaVersion":2,"name":"Named"}"#);
        assert_eq!(def.name, "Named");
        let def = parse("p", "{");
        assert_eq!(def.name, "p");
    }

    #[test]
    fn load_reads_json_files_sorted_and_creates_the_folder() {
        let base = std::env::temp_dir().join(format!("tg-store-{}", uuid::Uuid::new_v4()));
        let dir = base.join("processes");
        assert!(load(&dir).is_empty());
        assert!(dir.is_dir());

        fs::write(dir.join("b.json"), GOOD).unwrap();
        fs::write(dir.join("a.json"), "{").unwrap();
        fs::write(dir.join("notes.txt"), "ignore me").unwrap();
        fs::write(dir.join(".hidden.json"), GOOD).unwrap();
        fs::create_dir_all(dir.join("sub.json")).unwrap();

        let defs = load(&dir);
        let ids: Vec<&str> = defs.iter().map(|d| d.id.as_str()).collect();
        assert_eq!(ids, ["a", "b"]);
        assert!(defs[0].problem.is_some());
        assert!(defs[1].problem.is_none());
        let _ = fs::remove_dir_all(&base);
    }
}
```

10. Run `cd src-tauri && cargo fmt` and then the commands in Acceptance.

## Acceptance

- [ ] `cd src-tauri && cargo test -p terminal-grid state::` shows 13 tests, all ok (nine existing and four new)
- [ ] `cd src-tauri && cargo test -p terminal-grid triggers::store` shows 6 tests, all ok
- [ ] `cd src-tauri && cargo fmt --check` exits 0
- [ ] `cd src-tauri && cargo clippy -p terminal-grid --all-targets 2>&1 | grep -c warning` prints the same number or a lower one than the baseline in the Log (run it with `--all-targets` for the baseline too; the app crate has existing warnings and this card must not add any)
- [ ] `cd src-tauri && cargo test --workspace` passes
- [ ] `git status --porcelain` lists only paths under `files:` (the new `src-tauri/src/triggers/` folder counts as its two files)

## Out of scope

- Using `TriggersConfig` anywhere else. The service, commands and frontend type come in later cards.
- Adding `triggers` to the TypeScript `Config` type. T37 does that.
- Reading `$Ellipsis` variables. `store.rs` keeps `when` values exactly as written.
- Section 9 of `docs/technical-design.md`. Epic slice E documents the config block.
- Fixing existing clippy warnings in the app crate.

## If stuck

- If `cargo test` says a `Config { ... }` literal elsewhere is missing the field `triggers`, find it with `grep -rn "Config {" src-tauri/src` and add `triggers: TriggersConfig::default(),` only if the literal has no `..Default::default()`. If it is outside `state/mod.rs`, set status blocked and say where.
- If the `finished_color` lines in step 2 are not found exactly, the file changed since this card was written. Set status blocked and quote what is there.
- If `store.rs` tests fail on Windows because of path separators, they should not: the store tests use only file names. Set status blocked and quote the first failure.
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
