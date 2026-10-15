//! Tests for `find_gh`, `first_file` and `availability`.

use super::*;

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tg-github-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn the_first_folder_with_the_file_wins() {
    let (a, b, c) = (temp("find-a"), temp("find-b"), temp("find-c"));
    std::fs::write(b.join("gh"), "").unwrap();
    std::fs::write(c.join("gh"), "").unwrap();
    let got = first_file(&[a.clone(), b.clone(), c.clone()], "gh");
    assert_eq!(got, Some(b.join("gh")));
    for dir in [a, b, c] {
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[test]
fn a_folder_called_gh_is_not_the_program() {
    let a = temp("dir-a");
    std::fs::create_dir(a.join("gh")).unwrap();
    assert_eq!(first_file(std::slice::from_ref(&a), "gh"), None);
    assert_eq!(first_file(&[], "gh"), None);
    std::fs::remove_dir_all(a).unwrap();
}

#[test]
fn availability_follows_whether_gh_was_found() {
    match find_gh() {
        Some(path) => {
            assert!(path.is_file());
            assert_eq!(availability(), Availability::Available);
        }
        None => assert_eq!(
            availability(),
            Availability::Unsupported {
                reason: NOT_FOUND.into()
            }
        ),
    }
}

#[test]
fn the_lookup_is_remembered() {
    assert_eq!(find_gh(), find_gh());
}

#[test]
fn the_not_found_text_names_the_download_page() {
    assert!(NOT_FOUND.contains("https://cli.github.com"));
}
