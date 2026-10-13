//! Sandboxing and name validation for the notes root.
//! See docs/technical-design.md section 8.3.

use std::path::{Component, Path, PathBuf};

use crate::error::{AppError, Result};

/// Names Windows refuses, with or without an extension.
const RESERVED: [&str; 22] = [
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
    "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

const ILLEGAL: [char; 9] = ['<', '>', ':', '"', '/', '\\', '|', '?', '*'];

/// Joins a relative path to the root and refuses anything that escapes it.
///
/// The check is done on the lexical path first, so a path that does not exist
/// yet (a file about to be created) is validated too, and again on the
/// canonical form when the target exists, which is what catches symlinks.
pub fn resolve(root: &Path, rel_path: &str) -> Result<PathBuf> {
    let relative = Path::new(rel_path);
    if relative.is_absolute() || rel_path.contains(':') {
        return Err(outside(rel_path));
    }

    let mut resolved = root.to_path_buf();
    for component in relative.components() {
        match component {
            Component::Normal(segment) => {
                let name = segment.to_string_lossy();
                validate_name(&name)?;
                resolved.push(segment);
            }
            Component::CurDir => {}
            // `..`, a drive letter or a root all leave the sandbox.
            _ => return Err(outside(rel_path)),
        }
    }

    let canonical_root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    if let Ok(canonical) = resolved.canonicalize() {
        if !canonical.starts_with(&canonical_root) {
            return Err(outside(rel_path));
        }
    } else if !resolved.starts_with(root) {
        return Err(outside(rel_path));
    }

    Ok(resolved)
}

/// Path of `path` relative to `root`, with forward slashes.
pub fn relative_of(root: &Path, path: &Path) -> Option<String> {
    let relative = path.strip_prefix(root).ok()?;
    let text = relative.to_string_lossy().replace('\\', "/");
    (!text.is_empty()).then_some(text)
}

/// A single path segment: no separators, no reserved name, no trailing dot.
pub fn validate_name(name: &str) -> Result<()> {
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed == "." || trimmed == ".." {
        return Err(invalid(name, "the name is empty or reserved"));
    }
    if name
        .chars()
        .any(|c| ILLEGAL.contains(&c) || (c as u32) < 0x20)
    {
        return Err(invalid(name, r#"< > : " / \ | ? * are not allowed"#));
    }
    if name.ends_with('.') || name.ends_with(' ') {
        return Err(invalid(name, "names cannot end with a dot or a space"));
    }

    let stem = name
        .split_once('.')
        .map(|(stem, _)| stem)
        .unwrap_or(name)
        .to_ascii_lowercase();
    if RESERVED.contains(&stem.as_str()) {
        return Err(invalid(name, "this name is reserved by Windows"));
    }
    Ok(())
}

fn outside(rel_path: &str) -> AppError {
    AppError::OutsideRoot {
        message: format!("{rel_path} is outside the notes folder"),
    }
}

fn invalid(name: &str, why: &str) -> AppError {
    AppError::InvalidName {
        message: format!("{name}: {why}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> PathBuf {
        std::env::temp_dir().join("augry-tests-notes")
    }

    #[test]
    fn resolves_a_nested_relative_path() {
        let resolved = resolve(&root(), "agents/claude-code.md").expect("should resolve");
        assert!(
            resolved.ends_with("agents/claude-code.md")
                || resolved.ends_with(r"agents\claude-code.md")
        );
    }

    #[test]
    fn rejects_parent_traversal() {
        assert!(matches!(
            resolve(&root(), "../secrets.md"),
            Err(AppError::OutsideRoot { .. })
        ));
        assert!(matches!(
            resolve(&root(), "agents/../../secrets.md"),
            Err(AppError::OutsideRoot { .. })
        ));
    }

    #[test]
    fn rejects_absolute_paths() {
        assert!(matches!(
            resolve(&root(), "C:/Windows/system.ini"),
            Err(AppError::OutsideRoot { .. })
        ));
        assert!(matches!(
            resolve(&root(), "/etc/passwd"),
            Err(AppError::OutsideRoot { .. })
        ));
    }

    #[test]
    fn rejects_windows_reserved_names() {
        assert!(matches!(
            validate_name("CON.md"),
            Err(AppError::InvalidName { .. })
        ));
        assert!(matches!(
            validate_name("lpt9"),
            Err(AppError::InvalidName { .. })
        ));
        assert!(validate_name("console.md").is_ok());
    }

    #[test]
    fn rejects_illegal_characters_and_trailing_dots() {
        for name in ["a<b.md", "a|b.md", "a?b.md", "note.", "note "] {
            assert!(
                matches!(validate_name(name), Err(AppError::InvalidName { .. })),
                "{name} should be rejected"
            );
        }
    }

    #[test]
    fn accepts_ordinary_names() {
        for name in ["claude-code.md", "2026-09-07.md", "notes (draft).md"] {
            assert!(validate_name(name).is_ok(), "{name} should be accepted");
        }
    }

    #[test]
    fn relative_of_uses_forward_slashes() {
        let root = Path::new("C:/notes");
        let path = Path::new("C:/notes/agents/claude.md");
        assert_eq!(relative_of(root, path).as_deref(), Some("agents/claude.md"));
        assert_eq!(relative_of(root, root), None);
    }
}
