//! The pure half of running `gh`: the `Gh` trait the poll loop talks to, where
//! to look for the program, which arguments to give it and how to read the two
//! answers that are not a list (`gh auth status` and `gh api user`). No child
//! processes here: `os.rs` starts them.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use super::json::{text, Value};
use super::response::GhResponse;

/// What the poll loop needs from `gh`. `os::GhCli` runs the real program; the
/// tests use a fake that serves saved answers.
pub trait Gh: Send {
    /// One `GET`. `path` is relative to the API root, such as
    /// `repos/cli/cli/pulls?per_page=100`. `etag` is sent as `If-None-Match`.
    fn get(&self, path: &str, etag: Option<&str>) -> GhResponse;

    /// The login of the signed in user, from `GET user`. None when it cannot
    /// be read.
    fn login(&self) -> Option<String>;
}

/// The arguments of `gh auth status --json hosts`. It never prints a token and
/// always exits 0 unless something is badly wrong (R02).
pub const AUTH_ARGS: [&str; 4] = ["auth", "status", "--json", "hosts"];

/// The folders `gh` is installed to when the app's own PATH does not have them.
const UNIX_DIRS: [&str; 3] = ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"];
const WINDOWS_DIR: &str = r"C:\Program Files\GitHub CLI";

/// The file name of the program.
pub fn exe_name(windows: bool) -> &'static str {
    if windows {
        "gh.exe"
    } else {
        "gh"
    }
}

/// Where to look for `gh`, in order and without repeats: the folders of the
/// PATH (absolute ones only, so a `.` in it never runs a program from the
/// current folder), then the usual install folders. No login shell is asked.
pub fn candidate_dirs(
    path_var: Option<&OsStr>,
    home: Option<&Path>,
    windows: bool,
) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    let mut add = |dir: PathBuf| {
        if !dirs.contains(&dir) {
            dirs.push(dir);
        }
    };
    if let Some(var) = path_var {
        for dir in std::env::split_paths(var).filter(|d| d.is_absolute()) {
            add(dir);
        }
    }
    if windows {
        add(PathBuf::from(WINDOWS_DIR));
    } else {
        for dir in UNIX_DIRS {
            add(PathBuf::from(dir));
        }
        if let Some(home) = home {
            add(home.join(".local").join("bin"));
        }
    }
    dirs
}

/// The arguments of one request: `gh api -i [-H "If-None-Match: <etag>"] <path>`.
/// `-i` makes `gh` print the status line and headers on stdout, which is where
/// `parse_gh_output` reads them from.
pub fn api_args(path: &str, etag: Option<&str>) -> Vec<String> {
    let mut args = vec!["api".to_string(), "-i".to_string()];
    if let Some(etag) = etag {
        args.push("-H".to_string());
        args.push(format!("If-None-Match: {etag}"));
    }
    args.push(path.to_string());
    args
}

/// Reads the stdout of `gh auth status --json hosts`: true when `github.com`
/// has a login in state `success`. Anything else, including text that is not
/// JSON, is false.
pub fn signed_in(stdout: &str) -> bool {
    let hosts = serde_json::from_str::<Value>(stdout).ok().and_then(|v| {
        v.get("hosts")?
            .get("github.com")?
            .as_array()
            .map(|list| list.to_vec())
    });
    hosts
        .unwrap_or_default()
        .iter()
        .any(|login| text(login, "/state") == "success")
}

/// The `login` in the body of `GET user`.
pub fn login_from_body(body: &str) -> Option<String> {
    let user = serde_json::from_str::<Value>(body).ok()?;
    let login = text(&user, "/login");
    (!login.is_empty()).then(|| login.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn abs(name: &str) -> PathBuf {
        std::env::temp_dir().join(name)
    }

    #[test]
    fn the_program_name_depends_on_the_platform() {
        assert_eq!(exe_name(false), "gh");
        assert_eq!(exe_name(true), "gh.exe");
    }

    #[test]
    fn unix_looks_in_path_then_the_usual_folders() {
        let path = std::env::join_paths([abs("a"), abs("b")]).unwrap();
        let dirs = candidate_dirs(Some(&path), Some(Path::new("/home/u")), false);
        assert_eq!(
            dirs,
            [
                abs("a"),
                abs("b"),
                PathBuf::from("/opt/homebrew/bin"),
                PathBuf::from("/usr/local/bin"),
                PathBuf::from("/usr/bin"),
                Path::new("/home/u").join(".local").join("bin"),
            ]
        );
    }

    #[test]
    fn windows_looks_in_path_then_program_files() {
        let path = std::env::join_paths([abs("a")]).unwrap();
        let dirs = candidate_dirs(Some(&path), Some(Path::new("/home/u")), true);
        assert_eq!(
            dirs,
            [abs("a"), PathBuf::from(r"C:\Program Files\GitHub CLI")]
        );
    }

    #[test]
    fn relative_path_entries_and_repeats_are_dropped() {
        let path = std::env::join_paths([
            PathBuf::from("."),
            PathBuf::from("bin"),
            abs("a"),
            abs("a"),
            PathBuf::from("/usr/bin"),
        ])
        .unwrap();
        let dirs = candidate_dirs(Some(&path), None, false);
        assert_eq!(dirs.iter().filter(|d| **d == abs("a")).count(), 1);
        assert_eq!(
            dirs.iter()
                .filter(|d| d.as_path() == Path::new("/usr/bin"))
                .count(),
            1
        );
        assert!(dirs
            .iter()
            .all(|d| d != Path::new(".") && d != Path::new("bin")));
    }

    #[test]
    fn no_path_and_no_home_still_gives_the_usual_folders() {
        let dirs = candidate_dirs(None, None, false);
        assert_eq!(dirs.len(), 3);
        assert_eq!(dirs[0], PathBuf::from("/opt/homebrew/bin"));
    }

    #[test]
    fn api_args_have_the_etag_header_only_when_there_is_an_etag() {
        assert_eq!(
            api_args("repos/o/r/pulls?per_page=100", None),
            ["api", "-i", "repos/o/r/pulls?per_page=100"]
        );
        assert_eq!(
            api_args("repos/o/r/pulls", Some("W/\"093a24\"")),
            [
                "api",
                "-i",
                "-H",
                "If-None-Match: W/\"093a24\"",
                "repos/o/r/pulls"
            ]
        );
    }

    #[test]
    fn auth_args_never_ask_for_a_token() {
        assert_eq!(AUTH_ARGS.join(" "), "auth status --json hosts");
    }

    #[test]
    fn signed_in_needs_a_successful_github_com_login() {
        let yes = r#"{"hosts":{"github.com":[{"state":"success","active":true,"host":"github.com","login":"octocat"}]}}"#;
        assert!(signed_in(yes));
        let second = r#"{"hosts":{"github.com":[{"state":"error"},{"state":"success"}]}}"#;
        assert!(signed_in(second));
        assert!(!signed_in(r#"{"hosts":{}}"#));
        assert!(!signed_in(
            r#"{"hosts":{"github.com":[{"state":"error"}]}}"#
        ));
        assert!(!signed_in(
            r#"{"hosts":{"example.com":[{"state":"success"}]}}"#
        ));
        assert!(!signed_in(r#"{"hosts":{"github.com":"success"}}"#));
        assert!(!signed_in("You are not logged into any GitHub hosts."));
        assert!(!signed_in(""));
    }

    #[test]
    fn login_is_read_from_the_user_body() {
        assert_eq!(
            login_from_body(r#"{"login":"octocat","id":1}"#),
            Some("octocat".to_string())
        );
        assert_eq!(login_from_body(r#"{"login":""}"#), None);
        assert_eq!(login_from_body(r#"{"message":"Bad credentials"}"#), None);
        assert_eq!(login_from_body("not json"), None);
    }
}
