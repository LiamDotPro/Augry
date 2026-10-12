//! Variables in a trigger's `when` values: `${home}`, `${downloads}`,
//! `${notes.root}`, `${screenshots}` and `${repo.root}`. Design:
//! plans/001-trigger-core/epic.md Decision 12 and research note R05.
//!
//! The string parsing for the screenshots folder is in plain functions that
//! take strings, so their tests run on every OS. Only `screenshots_dir`
//! (further down) calls a program.

use std::path::{Path, PathBuf};

/// `~` alone or a `~/` prefix becomes `home`. Anything else is returned as is.
/// (`~user` is not handled.)
pub fn expand_tilde(value: &str, home: &Path) -> PathBuf {
    if value == "~" {
        return home.to_path_buf();
    }
    match value.strip_prefix("~/") {
        Some(rest) => home.join(rest),
        None => PathBuf::from(value),
    }
}

/// Tidies what `defaults read` prints: trims white space and a trailing `/`
/// (but keeps a lone `/`). None when nothing is left.
pub fn clean_location(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    let without_slash = trimmed.trim_end_matches('/');
    let tidy = if without_slash.is_empty() && trimmed.starts_with('/') {
        "/"
    } else {
        without_slash
    };
    (!tidy.is_empty()).then(|| tidy.to_string())
}

/// Where macOS saves screenshots. `raw` is the output of
/// `defaults read com.apple.screencapture location`, None when the command
/// failed (the setting was never changed). A value that is not an absolute
/// path counts as unset. Unset means the Desktop.
pub fn macos_location(raw: Option<&str>, home: &Path) -> PathBuf {
    raw.and_then(clean_location)
        .map(|value| expand_tilde(&value, home))
        .filter(|path| path.to_string_lossy().starts_with('/'))
        .unwrap_or_else(|| home.join("Desktop"))
}

/// The value part of the line `reg query ... /v <name>` prints for a string
/// value, for example
/// `    {B7BE...}    REG_EXPAND_SZ    %USERPROFILE%\Pictures\Screenshots`.
pub fn parse_reg_value(stdout: &str) -> Option<String> {
    for line in stdout.lines() {
        for token in ["REG_EXPAND_SZ", "REG_SZ"] {
            if let Some(position) = line.find(token) {
                let value = line[position + token.len()..].trim();
                if !value.is_empty() {
                    return Some(value.to_string());
                }
            }
        }
    }
    None
}

/// Replaces each `%NAME%` with `env(NAME)`. None when a name has no value.
/// A lone `%` with no partner is kept as text.
pub fn expand_percent(value: &str, env: &dyn Fn(&str) -> Option<String>) -> Option<String> {
    let mut out = String::new();
    let mut rest = value;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('%') {
            Some(end) => {
                out.push_str(&env(&after[..end])?);
                rest = &after[end + 1..];
            }
            None => {
                out.push('%');
                out.push_str(after);
                return Some(out);
            }
        }
    }
    out.push_str(rest);
    Some(out)
}

/// Where Windows saves screenshots. `reg_stdout` is the output of the
/// `reg query` for the Screenshots known folder, None when the command failed.
/// With no value the folder is `Pictures\Screenshots`.
pub fn windows_location(
    reg_stdout: Option<&str>,
    pictures: Option<PathBuf>,
    env: &dyn Fn(&str) -> Option<String>,
) -> Option<PathBuf> {
    reg_stdout
        .and_then(parse_reg_value)
        .and_then(|value| expand_percent(&value, env))
        .map(PathBuf::from)
        .or_else(|| pictures.map(|dir| dir.join("Screenshots")))
}

/// Where Linux desktops save screenshots: `Pictures/Screenshots` when that
/// folder exists, else `Pictures` when that exists.
pub fn linux_location(pictures: Option<PathBuf>) -> Option<PathBuf> {
    let pictures = pictures?;
    let screenshots = pictures.join("Screenshots");
    if screenshots.is_dir() {
        Some(screenshots)
    } else if pictures.is_dir() {
        Some(pictures)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn home() -> PathBuf {
        PathBuf::from("/Users/me")
    }

    #[test]
    fn a_tilde_becomes_the_home_folder() {
        assert_eq!(expand_tilde("~", &home()), home());
        assert_eq!(
            expand_tilde("~/Documents", &home()),
            home().join("Documents")
        );
        assert_eq!(expand_tilde("/tmp/x", &home()), PathBuf::from("/tmp/x"));
        assert_eq!(expand_tilde("~other/x", &home()), PathBuf::from("~other/x"));
    }

    #[test]
    fn locations_are_tidied() {
        assert_eq!(clean_location("~/Documents/\n"), Some("~/Documents".into()));
        assert_eq!(clean_location("/"), Some("/".into()));
        assert_eq!(clean_location("  \n"), None);
        assert_eq!(clean_location(""), None);
    }

    #[test]
    fn macos_reads_the_setting_or_falls_back_to_the_desktop() {
        let desktop = home().join("Desktop");
        assert_eq!(
            macos_location(Some("~/Documents/\n"), &home()),
            home().join("Documents")
        );
        assert_eq!(
            macos_location(Some("/Volumes/Shots\n"), &home()),
            PathBuf::from("/Volumes/Shots")
        );
        assert_eq!(macos_location(None, &home()), desktop);
        assert_eq!(macos_location(Some("\n"), &home()), desktop);
        assert_eq!(macos_location(Some("relative/dir"), &home()), desktop);
    }

    const REG: &str = "\r\nHKEY_CURRENT_USER\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Explorer\\User Shell Folders\r\n    {B7BEDE81-DF94-4682-A7D8-57A52620B86F}    REG_EXPAND_SZ    %USERPROFILE%\\Pictures\\Screenshots\r\n\r\n";

    #[test]
    fn reg_output_gives_the_value() {
        assert_eq!(
            parse_reg_value(REG),
            Some("%USERPROFILE%\\Pictures\\Screenshots".into())
        );
        assert_eq!(
            parse_reg_value("    name    REG_SZ    D:\\My Shots"),
            Some("D:\\My Shots".into())
        );
        assert_eq!(
            parse_reg_value("ERROR: The system was unable to find the specified registry key"),
            None
        );
        assert_eq!(parse_reg_value(""), None);
    }

    #[test]
    fn percent_names_are_expanded() {
        let env = |name: &str| match name {
            "USERPROFILE" => Some("C:\\Users\\me".to_string()),
            _ => None,
        };
        assert_eq!(
            expand_percent("%USERPROFILE%\\Pictures", &env),
            Some("C:\\Users\\me\\Pictures".into())
        );
        assert_eq!(expand_percent("D:\\plain", &env), Some("D:\\plain".into()));
        assert_eq!(expand_percent("%MISSING%\\x", &env), None);
        assert_eq!(expand_percent("50% off", &env), Some("50% off".into()));
    }

    #[test]
    fn windows_prefers_the_registry_then_pictures() {
        let env = |name: &str| (name == "USERPROFILE").then(|| "C:\\Users\\me".to_string());
        let pictures = Some(PathBuf::from("C:\\Users\\me\\Pictures"));
        assert_eq!(
            windows_location(Some(REG), pictures.clone(), &env),
            Some(PathBuf::from("C:\\Users\\me\\Pictures\\Screenshots"))
        );
        assert_eq!(
            windows_location(None, pictures.clone(), &env),
            Some(PathBuf::from("C:\\Users\\me\\Pictures").join("Screenshots"))
        );
        assert_eq!(windows_location(None, None, &env), None);
    }

    #[test]
    fn linux_prefers_the_screenshots_folder() {
        let base = std::env::temp_dir().join(format!("tg-shots-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&base).unwrap();
        assert_eq!(linux_location(Some(base.clone())), Some(base.clone()));
        std::fs::create_dir_all(base.join("Screenshots")).unwrap();
        assert_eq!(
            linux_location(Some(base.clone())),
            Some(base.join("Screenshots"))
        );
        assert_eq!(linux_location(Some(base.join("missing"))), None);
        assert_eq!(linux_location(None), None);
        let _ = std::fs::remove_dir_all(&base);
    }
}
