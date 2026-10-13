//! Variables in a trigger's `when` values: `${home}`, `${downloads}`,
//! `${notes.root}`, `${screenshots}`, `${repo.root}` and `${repo.remote}`. Design:
//! plans/001-trigger-core/epic.md Decision 12 and research note R05.
//!
//! The string parsing for the screenshots folder is in plain functions that
//! take strings, so their tests run on every OS. Only `screenshots_dir`
//! (further down) calls a program.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde_json::Value;

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

/// `owner/name` of a GitHub remote URL, None for any other host or shape.
/// Reads https, http, ssh and git URLs and the scp form
/// `git@github.com:owner/name.git`. A user name, password or token in the URL
/// is dropped and never returned.
pub fn github_slug(url: &str) -> Option<String> {
    let url = url.trim();
    let (host, path) = match url.split_once("://") {
        Some((scheme, rest)) => {
            let scheme = scheme.to_ascii_lowercase();
            if !matches!(scheme.as_str(), "https" | "http" | "ssh" | "git") {
                return None;
            }
            let (authority, path) = rest.split_once('/')?;
            let host = authority.rsplit('@').next()?;
            (host.split(':').next()?, path)
        }
        None => {
            let (authority, path) = url.split_once(':')?;
            (authority.rsplit('@').next()?, path)
        }
    };
    if !host.eq_ignore_ascii_case("github.com") {
        return None;
    }
    let path = path.trim_end_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    let (owner, name) = path.split_once('/')?;
    let word = |part: &str| {
        !part.is_empty()
            && part != "."
            && part != ".."
            && part
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    };
    (word(owner) && word(name)).then(|| format!("{owner}/{name}"))
}

/// Everything a variable can stand for. Built fresh for each refresh.
pub struct Context {
    pub home: PathBuf,
    pub downloads: Option<PathBuf>,
    pub notes_root: PathBuf,
    /// Repository roots of the open panes (`GitInfo.repo_root`).
    pub repo_roots: Vec<String>,
    /// `owner/name` of the GitHub origin remote of the open panes (see
    /// `github_slug`). Panes without a GitHub remote add nothing.
    pub repo_remotes: Vec<String>,
    /// Called only when a rule uses `${screenshots}`, because it may run a
    /// program. A function pointer so tests can pass a stub.
    pub screenshots: fn() -> Option<PathBuf>,
}

impl Context {
    pub fn detect(
        notes_root: PathBuf,
        repo_roots: Vec<String>,
        repo_remotes: Vec<String>,
    ) -> Context {
        Context {
            home: crate::pty::home_dir(),
            downloads: dirs::download_dir(),
            notes_root,
            repo_roots,
            repo_remotes,
            screenshots: screenshots_dir,
        }
    }
}

/// The screenshots folder, only when it exists.
#[cfg(target_os = "macos")]
fn screenshots_dir() -> Option<PathBuf> {
    let raw = std::process::Command::new("defaults")
        .args(["read", "com.apple.screencapture", "location"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok());
    let dir = macos_location(raw.as_deref(), &crate::pty::home_dir());
    dir.is_dir().then_some(dir)
}

/// The screenshots folder, only when it exists.
#[cfg(target_os = "windows")]
fn screenshots_dir() -> Option<PathBuf> {
    use std::os::windows::process::CommandExt;
    // CREATE_NO_WINDOW, so no console window flashes.
    const NO_WINDOW: u32 = 0x0800_0000;
    let stdout = std::process::Command::new("reg")
        .args([
            "query",
            r"HKCU\SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\User Shell Folders",
            "/v",
            "{B7BEDE81-DF94-4682-A7D8-57A52620B86F}",
        ])
        .creation_flags(NO_WINDOW)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).into_owned());
    let dir = windows_location(stdout.as_deref(), dirs::picture_dir(), &|name| {
        std::env::var(name).ok()
    })?;
    dir.is_dir().then_some(dir)
}

/// The screenshots folder, only when it exists.
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn screenshots_dir() -> Option<PathBuf> {
    linux_location(dirs::picture_dir())
}

/// The `when` of one rule after variables are replaced.
pub type When = BTreeMap<String, Value>;

pub enum Expanded {
    /// One entry per rule. The first part is the rule id suffix: empty, or
    /// `#1`, `#2` and so on when `${repo.root}` or `${repo.remote}` fans out over several repositories.
    Rules(Vec<(String, When)>),
    /// The rule cannot run on this machine right now; the text says why. It
    /// never contains a value from the user's disk.
    Skipped(String),
}

/// Replaces every variable in the string values of `when`, including strings
/// inside arrays and objects.
pub fn expand(when: &When, ctx: &Context) -> Expanded {
    let value = Value::Object(when.clone().into_iter().collect());
    let by_root = mentions(&value, REPO_ROOT);
    let by_remote = mentions(&value, REPO_REMOTE);
    if by_root && by_remote {
        return Expanded::Skipped("a rule cannot use both ${repo.root} and ${repo.remote}".into());
    }
    let fan_out = by_root || by_remote;

    // One entry per rule: the repository root, or the GitHub remote, or neither.
    let places: Vec<(Option<String>, Option<String>)> = if by_root {
        let distinct: BTreeSet<&String> = ctx.repo_roots.iter().collect();
        if distinct.is_empty() {
            return Expanded::Skipped("no open pane is in a git repository".into());
        }
        distinct
            .into_iter()
            .map(|root| (Some(root.clone()), None))
            .collect()
    } else if by_remote {
        let distinct: BTreeSet<&String> = ctx.repo_remotes.iter().collect();
        if distinct.is_empty() {
            return Expanded::Skipped("no open pane has a GitHub origin remote".into());
        }
        distinct
            .into_iter()
            .map(|remote| (None, Some(remote.clone())))
            .collect()
    } else {
        vec![(None, None)]
    };

    let mut rules = Vec::new();
    for (index, (root, remote)) in places.iter().enumerate() {
        match substitute(&value, ctx, root.as_deref(), remote.as_deref()) {
            Ok(Value::Object(map)) => {
                let suffix = if fan_out {
                    format!("#{}", index + 1)
                } else {
                    String::new()
                };
                rules.push((suffix, map.into_iter().collect()));
            }
            Ok(_) => return Expanded::Skipped("the rule could not be read".into()),
            Err(reason) => return Expanded::Skipped(reason),
        }
    }
    Expanded::Rules(rules)
}

const REPO_ROOT: &str = "${repo.root}";
const REPO_REMOTE: &str = "${repo.remote}";

fn mentions(value: &Value, needle: &str) -> bool {
    match value {
        Value::String(text) => text.contains(needle),
        Value::Array(items) => items.iter().any(|item| mentions(item, needle)),
        Value::Object(map) => map.values().any(|item| mentions(item, needle)),
        _ => false,
    }
}

fn substitute(
    value: &Value,
    ctx: &Context,
    root: Option<&str>,
    remote: Option<&str>,
) -> Result<Value, String> {
    Ok(match value {
        Value::String(text) => Value::String(replace_variables(text, ctx, root, remote)?),
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|item| substitute(item, ctx, root, remote))
                .collect::<Result<_, _>>()?,
        ),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, item)| Ok((key.clone(), substitute(item, ctx, root, remote)?)))
                .collect::<Result<_, String>>()?,
        ),
        other => other.clone(),
    })
}

fn replace_variables(
    text: &str,
    ctx: &Context,
    root: Option<&str>,
    remote: Option<&str>,
) -> Result<String, String> {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find("${") {
        let Some(length) = rest[start..].find('}') else {
            break;
        };
        out.push_str(&rest[..start]);
        let name = &rest[start + 2..start + length];
        let replacement = match name {
            "home" => ctx.home.to_string_lossy().into_owned(),
            "notes.root" => ctx.notes_root.to_string_lossy().into_owned(),
            "downloads" => ctx
                .downloads
                .as_ref()
                .map(|dir| dir.to_string_lossy().into_owned())
                .ok_or("this machine has no downloads folder")?,
            "screenshots" => (ctx.screenshots)()
                .map(|dir| dir.to_string_lossy().into_owned())
                .ok_or("no screenshots folder was found on this machine")?,
            "repo.root" => root
                .map(str::to_string)
                .ok_or("no open pane is in a git repository")?,
            "repo.remote" => remote
                .map(str::to_string)
                .ok_or("no open pane has a GitHub origin remote")?,
            other => return Err(format!("unknown variable ${{{other}}}")),
        };
        out.push_str(&replacement);
        rest = &rest[start + length + 1..];
    }
    out.push_str(rest);
    Ok(out)
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

#[cfg(test)]
mod expand_tests {
    use super::*;
    use serde_json::json;

    fn shots() -> Option<PathBuf> {
        Some(PathBuf::from("/shots"))
    }

    fn nothing() -> Option<PathBuf> {
        None
    }

    fn ctx(roots: &[&str]) -> Context {
        Context {
            home: PathBuf::from("/home/me"),
            downloads: Some(PathBuf::from("/home/me/Downloads")),
            notes_root: PathBuf::from("/notes"),
            repo_roots: roots.iter().map(|r| r.to_string()).collect(),
            repo_remotes: Vec::new(),
            screenshots: shots,
        }
    }

    fn when(value: Value) -> When {
        value.as_object().unwrap().clone().into_iter().collect()
    }

    fn only(expanded: Expanded) -> When {
        match expanded {
            Expanded::Rules(mut rules) => {
                assert_eq!(rules.len(), 1);
                let (suffix, when) = rules.remove(0);
                assert_eq!(suffix, "");
                when
            }
            Expanded::Skipped(reason) => panic!("skipped: {reason}"),
        }
    }

    fn skipped(expanded: Expanded) -> String {
        match expanded {
            Expanded::Skipped(reason) => reason,
            Expanded::Rules(_) => panic!("expected a skip"),
        }
    }

    #[test]
    fn plain_values_pass_through() {
        let input = when(json!({ "path": "/x", "settle_ms": 500, "ignore": ["a", "b"] }));
        assert_eq!(only(expand(&input, &ctx(&[]))), input);
    }

    #[test]
    fn each_variable_is_replaced() {
        let input = when(json!({
            "a": "${home}/x",
            "b": "${downloads}",
            "c": "${notes.root}/n",
            "d": "${screenshots}",
            "e": ["${home}", "plain"]
        }));
        let out = only(expand(&input, &ctx(&[])));
        assert_eq!(out["a"], json!("/home/me/x"));
        assert_eq!(out["b"], json!("/home/me/Downloads"));
        assert_eq!(out["c"], json!("/notes/n"));
        assert_eq!(out["d"], json!("/shots"));
        assert_eq!(out["e"], json!(["/home/me", "plain"]));
    }

    #[test]
    fn repo_root_makes_one_rule_per_distinct_root() {
        let input = when(json!({ "path": "${repo.root}/docs" }));
        match expand(&input, &ctx(&["/r/b", "/r/a", "/r/b"])) {
            Expanded::Rules(rules) => {
                assert_eq!(rules.len(), 2);
                assert_eq!(rules[0].0, "#1");
                assert_eq!(rules[0].1["path"], json!("/r/a/docs"));
                assert_eq!(rules[1].0, "#2");
                assert_eq!(rules[1].1["path"], json!("/r/b/docs"));
            }
            Expanded::Skipped(reason) => panic!("skipped: {reason}"),
        }
    }

    #[test]
    fn missing_values_skip_the_rule_with_a_reason() {
        let repo = when(json!({ "path": "${repo.root}" }));
        assert_eq!(
            skipped(expand(&repo, &ctx(&[]))),
            "no open pane is in a git repository"
        );

        let mut no_downloads = ctx(&[]);
        no_downloads.downloads = None;
        let downloads = when(json!({ "path": "${downloads}" }));
        assert!(skipped(expand(&downloads, &no_downloads)).contains("downloads"));

        let mut no_shots = ctx(&[]);
        no_shots.screenshots = nothing;
        let shots = when(json!({ "path": "${screenshots}" }));
        assert!(skipped(expand(&shots, &no_shots)).contains("screenshots"));
    }

    fn ctx_with_remotes(remotes: &[&str]) -> Context {
        let mut context = ctx(&[]);
        context.repo_remotes = remotes.iter().map(|r| r.to_string()).collect();
        context
    }

    #[test]
    fn repo_remote_makes_one_rule_per_distinct_remote() {
        let input = when(json!({ "repo": "${repo.remote}", "interval_s": 120 }));
        match expand(&input, &ctx_with_remotes(&["o/b", "o/a", "o/b"])) {
            Expanded::Rules(rules) => {
                assert_eq!(rules.len(), 2);
                assert_eq!(rules[0].0, "#1");
                assert_eq!(rules[0].1["repo"], json!("o/a"));
                assert_eq!(rules[0].1["interval_s"], json!(120));
                assert_eq!(rules[1].0, "#2");
                assert_eq!(rules[1].1["repo"], json!("o/b"));
            }
            Expanded::Skipped(reason) => panic!("skipped: {reason}"),
        }
    }

    #[test]
    fn one_remote_gives_one_rule_with_a_suffix() {
        let input = when(json!({ "repo": "${repo.remote}" }));
        match expand(&input, &ctx_with_remotes(&["o/a"])) {
            Expanded::Rules(rules) => {
                assert_eq!(rules.len(), 1);
                assert_eq!(rules[0].0, "#1");
            }
            Expanded::Skipped(reason) => panic!("skipped: {reason}"),
        }
    }

    #[test]
    fn repo_remote_without_a_github_remote_skips_the_rule() {
        let input = when(json!({ "repo": "${repo.remote}" }));
        // Open panes in repositories do not help: only roots are set here.
        assert_eq!(
            skipped(expand(&input, &ctx(&["/r/a"]))),
            "no open pane has a GitHub origin remote"
        );
    }

    #[test]
    fn repo_root_and_repo_remote_together_skip_the_rule() {
        let input = when(json!({ "repo": "${repo.remote}", "path": "${repo.root}/x" }));
        let mut both = ctx(&["/r/a"]);
        both.repo_remotes = vec!["o/a".into()];
        assert_eq!(
            skipped(expand(&input, &both)),
            "a rule cannot use both ${repo.root} and ${repo.remote}"
        );
    }

    #[test]
    fn an_unknown_variable_skips_the_rule() {
        let input = when(json!({ "path": "${nope}/x" }));
        assert_eq!(
            skipped(expand(&input, &ctx(&[]))),
            "unknown variable ${nope}"
        );
    }

    #[test]
    fn an_unclosed_brace_is_left_alone() {
        let input = when(json!({ "glob": "a${b" }));
        assert_eq!(only(expand(&input, &ctx(&[])))["glob"], json!("a${b"));
    }
}

#[cfg(test)]
mod slug_tests {
    use super::*;

    #[test]
    fn github_urls_give_owner_and_name() {
        for url in [
            "https://github.com/cli/cli",
            "https://github.com/cli/cli.git",
            "https://github.com/cli/cli/",
            "http://github.com/cli/cli.git",
            "https://GitHub.com/cli/cli.git",
            "https://user:ghp_secret@github.com/cli/cli.git",
            "git@github.com:cli/cli.git",
            "git@github.com:cli/cli",
            "ssh://git@github.com/cli/cli.git",
            "ssh://git@github.com:22/cli/cli.git",
            "git://github.com/cli/cli.git",
            "  https://github.com/cli/cli.git\n",
        ] {
            assert_eq!(github_slug(url), Some("cli/cli".into()), "{url}");
        }
        assert_eq!(
            github_slug("git@github.com:my-org/my_repo.js.git"),
            Some("my-org/my_repo.js".into())
        );
    }

    #[test]
    fn other_hosts_and_shapes_give_nothing() {
        for url in [
            "",
            "https://gitlab.com/cli/cli.git",
            "git@gitlab.com:cli/cli.git",
            "https://github.com.evil.example/cli/cli.git",
            "https://evil.example/github.com/cli/cli.git",
            "https://github.com/cli",
            "https://github.com/cli/",
            "https://github.com/cli/cli/pulls",
            "https://github.com/cli/cli extra",
            "https://github.com/../cli",
            "ftp://github.com/cli/cli.git",
            "file:///srv/git/cli.git",
            "/home/me/code/cli",
            "C:\\code\\cli",
            "../cli",
        ] {
            assert_eq!(github_slug(url), None, "{url}");
        }
    }

    #[test]
    fn a_token_in_the_url_never_reaches_the_answer() {
        let slug = github_slug("https://x-access-token:ghs_abc123@github.com/o/n.git").unwrap();
        assert_eq!(slug, "o/n");
        assert!(!slug.contains("ghs_"));
    }
}
