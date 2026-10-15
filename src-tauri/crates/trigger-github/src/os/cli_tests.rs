//! `GhCli` and `auth_ok` against a stand in for `gh`: a shell script that
//! prints what `gh api -i` and `gh auth status` print. Unix only.

#![cfg(unix)]

use super::*;
use std::os::unix::fs::PermissionsExt;

/// Writes `gh` into a fresh folder and returns the folder and the file.
fn fake_gh(name: &str, script: &str) -> (PathBuf, PathBuf) {
    let dir = std::env::temp_dir().join(format!("tg-github-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let exe = dir.join("gh");
    std::fs::write(&exe, format!("#!/bin/sh\n{script}\n")).unwrap();
    std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
    (dir, exe)
}

#[test]
fn a_200_is_parsed_and_gh_is_given_the_right_arguments_and_environment() {
    let _guard = spawn_lock();
    let (dir, exe) = fake_gh(
        "get-200",
        r#"printf 'HTTP/2.0 200 OK\r\nEtag: "e1"\r\n\r\n%s|%s|%s' "$GH_PROMPT_DISABLED" "$GH_NO_UPDATE_NOTIFIER" "$*""#,
    );
    let got = GhCli::at(&exe).get("repos/o/r/pulls?per_page=100", Some("W/\"a\""));
    std::fs::remove_dir_all(dir).unwrap();
    match got {
        GhResponse::Ok { etag, body, .. } => {
            assert_eq!(etag.as_deref(), Some("\"e1\""));
            assert_eq!(
                body,
                "1|1|api -i -H If-None-Match: W/\"a\" repos/o/r/pulls?per_page=100"
            );
        }
        other => panic!("expected Ok, got {other:?}"),
    }
}

#[test]
fn a_304_that_gh_reports_with_exit_1_is_not_modified() {
    let _guard = spawn_lock();
    let (dir, exe) = fake_gh(
        "get-304",
        r#"printf 'HTTP/2.0 304 Not Modified\r\nEtag: "e1"\r\n\r\n'; exit 1"#,
    );
    let got = GhCli::at(&exe).get("repos/o/r/pulls", Some("\"e1\""));
    std::fs::remove_dir_all(dir).unwrap();
    assert!(matches!(got, GhResponse::NotModified { .. }), "{got:?}");
}

#[test]
fn no_login_is_the_not_logged_in_transport_error() {
    let _guard = spawn_lock();
    let (dir, exe) = fake_gh("get-exit4", "echo 'run gh auth login' >&2; exit 4");
    let got = GhCli::at(&exe).get("user", None);
    std::fs::remove_dir_all(dir).unwrap();
    assert_eq!(
        got,
        GhResponse::Transport(crate::logic::response::NOT_LOGGED_IN.into())
    );
}

#[test]
fn a_program_that_cannot_start_is_a_transport_error() {
    let _guard = spawn_lock();
    let missing = std::env::temp_dir().join("tg-github-missing-gh");
    let got = GhCli::at(&missing).get("user", None);
    match got {
        GhResponse::Transport(message) => assert!(message.starts_with("could not run gh")),
        other => panic!("expected Transport, got {other:?}"),
    }
}

#[test]
fn login_reads_the_user_and_gives_none_for_anything_else() {
    let _guard = spawn_lock();
    let (dir, exe) = fake_gh(
        "login-ok",
        r#"printf 'HTTP/2.0 200 OK\r\n\r\n{"login":"octocat"}'"#,
    );
    assert_eq!(GhCli::at(&exe).login(), Some("octocat".to_string()));
    std::fs::remove_dir_all(dir).unwrap();

    let (dir, exe) = fake_gh(
        "login-401",
        r#"printf 'HTTP/2.0 401 Unauthorized\r\n\r\n{"message":"Bad credentials"}'; exit 1"#,
    );
    assert_eq!(GhCli::at(&exe).login(), None);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn auth_ok_asks_for_hosts_json_and_reads_the_state() {
    let _guard = spawn_lock();
    let check = r#"[ "$*" = "auth status --json hosts" ] || exit 9"#;
    let (dir, exe) = fake_gh(
        "auth-yes",
        &format!(
            "{check}\nprintf '%s' '{}'",
            r#"{"hosts":{"github.com":[{"state":"success","login":"octocat"}]}}"#
        ),
    );
    assert_eq!(auth_ok(&exe), Ok(true));
    std::fs::remove_dir_all(dir).unwrap();

    let (dir, exe) = fake_gh(
        "auth-no",
        &format!("{check}\nprintf '%s' '{{\"hosts\":{{}}}}'"),
    );
    assert_eq!(auth_ok(&exe), Ok(false));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn auth_ok_is_an_error_when_the_question_cannot_be_answered() {
    let _guard = spawn_lock();
    let (dir, exe) = fake_gh("auth-exit1", "exit 1");
    let got = auth_ok(&exe);
    std::fs::remove_dir_all(dir).unwrap();
    assert_eq!(got, Err("gh auth status exited with code 1".to_string()));

    let missing = std::env::temp_dir().join("tg-github-missing-gh");
    let got = auth_ok(&missing);
    assert!(got.unwrap_err().starts_with("could not run gh"));
}
