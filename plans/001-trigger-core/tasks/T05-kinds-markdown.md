---
id: T05
epic: 001-trigger-core
title: Add kinds_markdown with an exact output test
status: todo
depends_on: [T04]
size: S
model: haiku
files:
  - src-tauri/crates/trigger-core/src/lib.rs
---

## Goal

`trigger_core::kinds_markdown(&[KindInfo]) -> String` renders the kinds table that every source crate's README must contain. A test pins the exact output.

## Read first

- `src-tauri/crates/trigger-core/src/lib.rs` the file you edit (T01 to T04)

## Steps

1. Insert this after the `Source` trait and before the first `#[cfg(test)]` module. The format is fixed: a header row, a separator row, then one row per kind, every line ending in `\n`. Cells are separated by ` | ` inside `| ... |`. Platforms are `macOS, Windows, Linux` (only those set, in that order, `none` when empty). Permission is `none` when `None`. When and payload are the field names joined with `, `, a required `when` field gets a `*` after its name, and an empty list is `none`. Payload fields never get a `*`.

```rust
/// Renders a kinds table for a README. Checked by each crate's tests/readme.rs.
pub fn kinds_markdown(kinds: &[KindInfo]) -> String {
    let mut out = String::from("| Kind | Title | Platforms | Permission | When | Payload |\n");
    out.push_str("| --- | --- | --- | --- | --- | --- |\n");
    for k in kinds {
        let permission = k.permission.map_or("none".to_string(), permission_label);
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} |\n",
            k.id,
            k.title,
            platforms_label(k.platforms),
            permission,
            fields_label(k.when, true),
            fields_label(k.payload, false),
        ));
    }
    out
}

fn platforms_label(p: Platforms) -> String {
    let all = [
        (Platforms::MACOS, "macOS"),
        (Platforms::WINDOWS, "Windows"),
        (Platforms::LINUX, "Linux"),
    ];
    let names: Vec<&str> = all.iter().filter(|(f, _)| p.has(*f)).map(|(_, n)| *n).collect();
    if names.is_empty() {
        "none".to_string()
    } else {
        names.join(", ")
    }
}

fn permission_label(p: Permission) -> String {
    match p {
        Permission::Accessibility => "accessibility".to_string(),
        Permission::ScreenRecording => "screen recording".to_string(),
        Permission::Location => "location".to_string(),
        Permission::Calendar => "calendar".to_string(),
        Permission::Reminders => "reminders".to_string(),
        Permission::Folder(name) => format!("folder {name}"),
        Permission::Microphone => "microphone".to_string(),
        Permission::Camera => "camera".to_string(),
        Permission::Notifications => "notifications".to_string(),
    }
}

fn fields_label(fields: &[FieldInfo], mark_required: bool) -> String {
    if fields.is_empty() {
        return "none".to_string();
    }
    let names: Vec<String> = fields
        .iter()
        .map(|f| if mark_required && f.required { format!("{}*", f.name) } else { f.name.to_string() })
        .collect();
    names.join(", ")
}
```

2. Append this test module at the very end of the file. Copy the expected string exactly; do not "fix" it.

```rust
#[cfg(test)]
mod markdown_tests {
    use super::*;

    const WHEN: &[FieldInfo] = &[
        FieldInfo { name: "path", doc: "Folder to watch", required: true },
        FieldInfo { name: "glob", doc: "Which files", required: false },
    ];
    const PAYLOAD: &[FieldInfo] = &[
        FieldInfo { name: "path", doc: "Where", required: true },
        FieldInfo { name: "name", doc: "File name", required: true },
    ];
    const KINDS: &[KindInfo] = &[
        KindInfo {
            id: "fs.file-saved",
            title: "File saved",
            summary: "A file was saved",
            platforms: Platforms::ALL,
            permission: Some(Permission::Folder("Downloads")),
            when: WHEN,
            payload: PAYLOAD,
        },
        KindInfo {
            id: "x.thing",
            title: "Thing",
            summary: "s",
            platforms: Platforms::MACOS.or(Platforms::LINUX),
            permission: None,
            when: &[],
            payload: &[],
        },
    ];

    #[test]
    fn renders_the_exact_table() {
        let expected = "\
| Kind | Title | Platforms | Permission | When | Payload |
| --- | --- | --- | --- | --- | --- |
| fs.file-saved | File saved | macOS, Windows, Linux | folder Downloads | path*, glob | path, name |
| x.thing | Thing | macOS, Linux | none | none | none |
";
        assert_eq!(kinds_markdown(KINDS), expected);
    }

    #[test]
    fn no_kinds_gives_only_the_header() {
        assert_eq!(
            kinds_markdown(&[]),
            "| Kind | Title | Platforms | Permission | When | Payload |\n| --- | --- | --- | --- | --- | --- |\n"
        );
    }
}
```

3. Run `cd src-tauri && cargo fmt -p trigger-core`, then the acceptance commands. If `cargo fmt` rewraps the test strings, that is fine, but the expected table text must stay byte for byte as in step 2.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-core markdown` passes (2 tests)
- [ ] `cd src-tauri && cargo test -p trigger-core` passes
- [ ] `cd src-tauri && cargo clippy -p trigger-core --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-core --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0
- [ ] `git status --porcelain` lists only paths under `files:`

## Out of scope

- Backticks around ids, a Summary column, or any other change to the format. Source crates' README tests compare against this exact output.
- Making the helper functions `pub`.

## If stuck

- If the exact table test fails, the code differs from step 1; do not edit the expected string.
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
