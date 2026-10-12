use super::*;
use crate::rule::parse;
use serde_json::json;
use std::cell::RefCell;
use std::rc::Rc;
use trigger_core::testkit::rule;

/// A rule, its state and a fake disk (the set of files that exist).
struct Fx {
    armed: Armed,
    state: State,
    disk: Rc<RefCell<HashSet<PathBuf>>>,
}

impl Fx {
    /// `existing` are paths relative to the root that exist and are known
    /// before the test starts, as after the start scan.
    fn new(kind: &str, extra_when: serde_json::Value, existing: &[&str]) -> Fx {
        let root = std::env::temp_dir().join("tg-fs-logic-root");
        let mut when = json!({ "path": root });
        when.as_object_mut()
            .unwrap()
            .extend(extra_when.as_object().unwrap().clone());
        let parsed = parse(&rule("p/t", kind, when)).unwrap();
        let armed = Armed {
            id: "p/t".into(),
            kind: kind.into(),
            root: root.clone(),
            when: parsed.when,
            glob: parsed.glob,
            ignore: parsed.ignore,
        };
        let files: HashSet<PathBuf> = existing.iter().map(|r| root.join(r)).collect();
        let mut state = State::default();
        state.seed(files.iter().cloned());
        Fx {
            armed,
            state,
            disk: Rc::new(RefCell::new(files)),
        }
    }

    fn saved() -> Fx {
        Fx::new("fs.file-saved", json!({}), &[])
    }

    fn p(&self, rel: &str) -> PathBuf {
        self.armed.root.join(rel)
    }

    fn put(&self, rel: &str) {
        self.disk.borrow_mut().insert(self.p(rel));
    }

    fn delete(&self, rel: &str) {
        self.disk.borrow_mut().remove(&self.p(rel));
    }

    fn raw(&mut self, at: u64, kind: RawKind, rels: &[&str]) {
        let paths = rels.iter().map(|r| self.p(r)).collect();
        let signal = Signal::Raw { at, kind, paths };
        assert!(self.observe(signal).is_empty());
    }

    fn tick(&mut self, now: u64) -> Vec<Obs> {
        self.observe(Signal::Tick { now })
    }

    fn step(&mut self, signal: Signal) -> Vec<(String, Payload)> {
        let disk = self.disk.clone();
        let is_file = move |p: &Path| disk.borrow().contains(p);
        step(&mut self.state, &self.armed, signal, &is_file)
    }

    fn observe(&mut self, signal: Signal) -> Vec<Obs> {
        let disk = self.disk.clone();
        let is_file = move |p: &Path| disk.borrow().contains(p);
        observe(&mut self.state, &self.armed, signal, &is_file)
    }
}

#[test]
fn macos_plain_save_of_a_known_file_is_one_saved() {
    // R03: Create, Metadata, Metadata, Data on a path that existed.
    let mut fx = Fx::new("fs.file-saved", json!({}), &["a.txt"]);
    fx.raw(1000, RawKind::Create, &["a.txt"]);
    fx.raw(1000, RawKind::Modify, &["a.txt"]);
    fx.raw(1001, RawKind::Modify, &["a.txt"]);
    fx.raw(1001, RawKind::Modify, &["a.txt"]);
    assert!(fx.tick(1100).is_empty(), "still inside the settle time");
    assert_eq!(fx.tick(1501), vec![Obs::Saved(fx.p("a.txt"))]);
    assert!(fx.tick(2000).is_empty(), "decided once");
}

#[test]
fn macos_atomic_save_is_one_saved_and_nothing_for_the_temp_file() {
    // R03: a.txt.tmp gets Create, Name, Metadata, Data; a.txt gets Name twice.
    let mut fx = Fx::new("fs.file-saved", json!({}), &["a.txt"]);
    fx.put("a.txt.tmp");
    fx.raw(1000, RawKind::Create, &["a.txt.tmp"]);
    fx.raw(1000, RawKind::Name, &["a.txt.tmp"]);
    fx.raw(1000, RawKind::Modify, &["a.txt.tmp"]);
    fx.raw(1001, RawKind::Name, &["a.txt"]);
    fx.raw(1001, RawKind::Name, &["a.txt"]);
    fx.delete("a.txt.tmp"); // renamed over the target
    assert_eq!(fx.tick(1600), vec![Obs::Saved(fx.p("a.txt"))]);
}

#[test]
fn a_new_file_is_created_even_after_several_quick_writes() {
    let mut fx = Fx::new("fs.file-created", json!({}), &[]);
    fx.put("new.txt");
    fx.raw(1000, RawKind::Create, &["new.txt"]);
    fx.raw(1100, RawKind::Modify, &["new.txt"]);
    fx.raw(1200, RawKind::Modify, &["new.txt"]);
    assert!(fx.tick(1600).is_empty(), "the last write was 400 ms ago");
    assert_eq!(fx.tick(1700), vec![Obs::Created(fx.p("new.txt"))]);
}

#[test]
fn a_file_that_existed_before_start_is_not_created() {
    let mut fx = Fx::new("fs.file-created", json!({}), &["old.txt"]);
    fx.raw(1000, RawKind::Create, &["old.txt"]);
    assert_eq!(fx.tick(2000), vec![Obs::Saved(fx.p("old.txt"))]);
}

#[test]
fn delete_then_recreate_is_one_saved() {
    let mut fx = Fx::new("fs.file-saved", json!({}), &["a.txt"]);
    fx.raw(1000, RawKind::Create, &["a.txt"]);
    fx.raw(1000, RawKind::Remove, &["a.txt"]);
    fx.raw(1001, RawKind::Create, &["a.txt"]);
    fx.raw(1001, RawKind::Modify, &["a.txt"]);
    assert_eq!(fx.tick(1600), vec![Obs::Saved(fx.p("a.txt"))]);
}

#[test]
fn a_deleted_file_is_removed_and_then_created_again_later() {
    let mut fx = Fx::new("fs.file-removed", json!({}), &["a.txt"]);
    fx.delete("a.txt");
    fx.raw(1000, RawKind::Remove, &["a.txt"]);
    fx.raw(1000, RawKind::Name, &["a.txt"]);
    assert_eq!(fx.tick(1600), vec![Obs::Removed(fx.p("a.txt"))]);
    fx.put("a.txt");
    fx.raw(3000, RawKind::Create, &["a.txt"]);
    assert_eq!(fx.tick(3600), vec![Obs::Created(fx.p("a.txt"))]);
}

#[test]
fn a_file_that_came_and_went_is_dropped() {
    let mut fx = Fx::saved();
    fx.raw(1000, RawKind::Create, &["swap.txt"]);
    assert!(fx.tick(2000).is_empty());
}

#[test]
fn directories_are_never_reported() {
    // The fake disk only holds files, so a directory is "not a file".
    let mut fx = Fx::saved();
    fx.raw(1000, RawKind::Create, &["subdir"]);
    assert!(fx.tick(2000).is_empty());
}

#[test]
fn access_events_and_paths_outside_the_root_are_ignored() {
    let mut fx = Fx::new("fs.file-saved", json!({}), &["a.txt"]);
    fx.raw(1000, RawKind::Ignore, &["a.txt"]);
    let outside = std::env::temp_dir().join("tg-fs-elsewhere").join("a.txt");
    let signal = Signal::Raw {
        at: 1000,
        kind: RawKind::Modify,
        paths: vec![outside, fx.armed.root.clone()],
    };
    assert!(fx.observe(signal).is_empty());
    assert!(fx.tick(2000).is_empty());
}

#[test]
fn settle_ms_is_per_rule() {
    let mut fx = Fx::new("fs.file-created", json!({ "settle_ms": 50 }), &[]);
    fx.put("a.txt");
    fx.raw(1000, RawKind::Create, &["a.txt"]);
    assert!(fx.tick(1049).is_empty());
    assert_eq!(fx.tick(1050), vec![Obs::Created(fx.p("a.txt"))]);
}

#[test]
fn a_rescan_forgets_pending_paths_and_asks_for_a_new_scan() {
    let mut fx = Fx::new("fs.file-saved", json!({}), &["a.txt"]);
    fx.raw(1000, RawKind::Modify, &["a.txt"]);
    fx.raw(1001, RawKind::Rescan, &[]);
    assert!(fx.tick(2000).is_empty());
    assert!(fx.state.take_reseed());
    assert!(!fx.state.take_reseed(), "only once");
    fx.state.seed([fx.p("b.txt")]);
    fx.put("b.txt");
    fx.raw(3000, RawKind::Modify, &["b.txt"]);
    assert_eq!(fx.tick(4000), vec![Obs::Saved(fx.p("b.txt"))]);
}

#[test]
fn glob_and_ignore_apply_when_emitting() {
    let mut fx = Fx::new(
        "fs.file-created",
        json!({ "glob": "**/*.md", "ignore": ["drafts/**"] }),
        &[],
    );
    for rel in ["a.md", "b.txt", "drafts/c.md", "d.md.tmp", ".git/e.md"] {
        fx.put(rel);
        fx.raw(1000, RawKind::Create, &[rel]);
    }
    assert_eq!(fx.tick(2000), vec![Obs::Created(fx.p("a.md"))]);
}

#[test]
fn step_keeps_only_the_rules_kind_and_builds_the_payload() {
    let mut fx = Fx::new("fs.file-saved", json!({}), &["docs/Plan.md"]);
    fx.raw(1000, RawKind::Modify, &["docs/Plan.md"]);
    let out = fx.step(Signal::Tick { now: 2000 });
    assert_eq!(out.len(), 1);
    let (kind, payload) = &out[0];
    assert_eq!(kind, "fs.file-saved");
    assert_eq!(payload["name"], json!("Plan.md"));
    assert_eq!(payload["relative"], json!("docs/Plan.md"));
    assert_eq!(payload["ext"], json!("md"));
    assert_eq!(
        payload["path"],
        json!(fx.p("docs/Plan.md").to_string_lossy())
    );
    assert!(!payload.contains_key("from"));

    // The same change is nothing to a rule of another kind.
    let mut other = Fx::new("fs.file-created", json!({}), &["docs/Plan.md"]);
    other.raw(1000, RawKind::Modify, &["docs/Plan.md"]);
    assert!(other.step(Signal::Tick { now: 2000 }).is_empty());
}

#[test]
fn a_file_without_an_extension_has_an_empty_ext() {
    let fx = Fx::saved();
    let payload = Obs::Saved(fx.p("Makefile")).payload(&fx.armed);
    assert_eq!(payload["ext"], json!(""));
    assert_eq!(payload["relative"], json!("Makefile"));
}

#[test]
fn windows_rename_from_and_to_is_one_renamed() {
    let mut fx = Fx::new("fs.file-renamed", json!({}), &["a.txt"]);
    fx.delete("a.txt");
    fx.put("b.txt");
    fx.raw(1000, RawKind::Name, &["a.txt"]);
    fx.raw(1000, RawKind::Name, &["b.txt"]);
    assert_eq!(
        fx.tick(1600),
        vec![Obs::Renamed {
            from: fx.p("a.txt"),
            to: fx.p("b.txt")
        }]
    );
}

#[test]
fn macos_rename_into_a_subfolder_is_one_renamed() {
    // R03: one Name(Any) per path, no pairing in the kind.
    let mut fx = Fx::new("fs.file-renamed", json!({}), &["a.txt"]);
    fx.delete("a.txt");
    fx.put("sub/a.txt");
    fx.raw(1000, RawKind::Name, &["a.txt"]);
    fx.raw(1000, RawKind::Name, &["sub/a.txt"]);
    let out = fx.tick(1600);
    assert_eq!(
        out,
        vec![Obs::Renamed {
            from: fx.p("a.txt"),
            to: fx.p("sub/a.txt")
        }]
    );
    let payload = out[0].payload(&fx.armed);
    assert_eq!(payload["from"], json!(fx.p("a.txt").to_string_lossy()));
    assert_eq!(payload["relative"], json!("sub/a.txt"));
}

#[test]
fn linux_rename_with_both_is_one_renamed() {
    let mut fx = Fx::new("fs.file-renamed", json!({}), &["a.txt"]);
    fx.delete("a.txt");
    fx.put("b.txt");
    fx.raw(1000, RawKind::Name, &["a.txt"]);
    fx.raw(1000, RawKind::Name, &["b.txt"]);
    fx.raw(1000, RawKind::Both, &["a.txt", "b.txt"]);
    assert_eq!(
        fx.tick(1600),
        vec![Obs::Renamed {
            from: fx.p("a.txt"),
            to: fx.p("b.txt")
        }]
    );
    assert!(fx.state.pairs.is_empty());
}

#[test]
fn both_pairs_two_removes_and_creates_without_guessing() {
    let mut fx = Fx::new("fs.file-renamed", json!({}), &["a.txt", "c.txt"]);
    fx.delete("a.txt");
    fx.delete("c.txt");
    fx.put("b.txt");
    fx.put("d.txt");
    fx.raw(1000, RawKind::Both, &["a.txt", "b.txt"]);
    fx.raw(1000, RawKind::Both, &["c.txt", "d.txt"]);
    let out = fx.tick(1600);
    assert_eq!(out.len(), 2);
    assert!(out.contains(&Obs::Renamed {
        from: fx.p("a.txt"),
        to: fx.p("b.txt")
    }));
    assert!(out.contains(&Obs::Renamed {
        from: fx.p("c.txt"),
        to: fx.p("d.txt")
    }));
}

#[test]
fn linux_atomic_save_with_both_is_one_saved() {
    // R03: Create temp, Data temp, Name(From) temp, Name(To) target,
    // Name(Both) [temp, target].
    let mut fx = Fx::new("fs.file-saved", json!({}), &["a.txt"]);
    fx.put("a.txt.swp-new");
    fx.raw(1000, RawKind::Create, &["a.txt.swp-new"]);
    fx.raw(1000, RawKind::Modify, &["a.txt.swp-new"]);
    fx.delete("a.txt.swp-new");
    fx.raw(1001, RawKind::Name, &["a.txt.swp-new"]);
    fx.raw(1001, RawKind::Name, &["a.txt"]);
    fx.raw(1001, RawKind::Both, &["a.txt.swp-new", "a.txt"]);
    assert_eq!(fx.tick(1600), vec![Obs::Saved(fx.p("a.txt"))]);
    assert!(fx.state.pairs.is_empty(), "the pair is forgotten");
}

#[test]
fn a_delete_and_an_unrelated_create_are_not_a_rename() {
    // macOS puts a stale rename flag on a deleted path, but a new file
    // has none.
    let mut fx = Fx::new("fs.file-renamed", json!({}), &["a.txt"]);
    fx.delete("a.txt");
    fx.put("b.txt");
    fx.raw(1000, RawKind::Remove, &["a.txt"]);
    fx.raw(1000, RawKind::Name, &["a.txt"]);
    fx.raw(1000, RawKind::Create, &["b.txt"]);
    assert_eq!(
        fx.tick(1600),
        vec![Obs::Removed(fx.p("a.txt")), Obs::Created(fx.p("b.txt"))]
    );
}

#[test]
fn two_removed_and_one_created_stay_separate() {
    let mut fx = Fx::new("fs.file-renamed", json!({}), &["a.txt", "b.txt"]);
    fx.delete("a.txt");
    fx.delete("b.txt");
    fx.put("c.txt");
    for rel in ["a.txt", "b.txt", "c.txt"] {
        fx.raw(1000, RawKind::Name, &[rel]);
    }
    let out = fx.tick(1600);
    assert_eq!(out.len(), 3);
    assert!(out.iter().all(|o| !matches!(o, Obs::Renamed { .. })));
}

#[test]
fn renaming_to_an_ignored_name_is_only_a_removal() {
    let mut fx = Fx::new("fs.file-renamed", json!({}), &["a.txt"]);
    fx.delete("a.txt");
    fx.put("a.txt~");
    fx.raw(1000, RawKind::Name, &["a.txt"]);
    fx.raw(1000, RawKind::Name, &["a.txt~"]);
    assert_eq!(fx.tick(1600), vec![Obs::Removed(fx.p("a.txt"))]);
}

#[test]
fn chrome_download_is_created_and_download_finished() {
    // x.zip.crdownload is written for a while, then renamed to x.zip.
    let mut fx = Fx::new("fs.download-finished", json!({}), &[]);
    fx.put("x.zip.crdownload");
    fx.raw(1000, RawKind::Create, &["x.zip.crdownload"]);
    fx.raw(3000, RawKind::Modify, &["x.zip.crdownload"]);
    assert!(fx.tick(3600).is_empty(), "the partial file is ignored");
    fx.raw(5000, RawKind::Modify, &["x.zip.crdownload"]);
    fx.delete("x.zip.crdownload");
    fx.put("x.zip");
    fx.raw(6000, RawKind::Name, &["x.zip.crdownload"]);
    fx.raw(6000, RawKind::Name, &["x.zip"]);
    assert_eq!(
        fx.tick(6600),
        vec![
            Obs::Created(fx.p("x.zip")),
            Obs::DownloadFinished(fx.p("x.zip"))
        ]
    );
    assert!(fx.tick(7000).is_empty());
}

#[test]
fn a_finished_download_over_an_existing_file_is_saved_and_finished() {
    let mut fx = Fx::new("fs.download-finished", json!({}), &["x.zip"]);
    fx.put("x.zip.part");
    fx.raw(1000, RawKind::Create, &["x.zip.part"]);
    fx.delete("x.zip.part");
    fx.raw(2000, RawKind::Name, &["x.zip.part"]);
    fx.raw(2000, RawKind::Name, &["x.zip"]);
    assert_eq!(
        fx.tick(2600),
        vec![
            Obs::Saved(fx.p("x.zip")),
            Obs::DownloadFinished(fx.p("x.zip"))
        ]
    );
}

#[test]
fn partial_suffixes_match_in_any_case() {
    let mut fx = Fx::new("fs.download-finished", json!({}), &[]);
    fx.put("Photo.JPG.CRDOWNLOAD");
    fx.raw(1000, RawKind::Create, &["Photo.JPG.CRDOWNLOAD"]);
    fx.delete("Photo.JPG.CRDOWNLOAD");
    fx.put("Photo.JPG");
    fx.raw(2000, RawKind::Name, &["Photo.JPG"]);
    let out = fx.tick(2600);
    assert!(out.contains(&Obs::DownloadFinished(fx.p("Photo.JPG"))));
}

#[test]
fn an_old_partial_does_not_make_a_download() {
    let mut fx = Fx::new("fs.download-finished", json!({}), &[]);
    fx.put("x.zip.crdownload");
    fx.raw(1000, RawKind::Create, &["x.zip.crdownload"]);
    fx.delete("x.zip.crdownload");
    assert!(fx.tick(2000).is_empty());
    fx.put("x.zip");
    fx.raw(20_000, RawKind::Create, &["x.zip"]);
    assert_eq!(fx.tick(20_600), vec![Obs::Created(fx.p("x.zip"))]);
}

#[test]
fn a_partial_that_still_exists_is_not_finished() {
    let mut fx = Fx::new("fs.download-finished", json!({}), &[]);
    fx.put("x.zip.part");
    fx.put("x.zip");
    fx.raw(1000, RawKind::Create, &["x.zip.part"]);
    fx.raw(1000, RawKind::Create, &["x.zip"]);
    assert_eq!(fx.tick(1600), vec![Obs::Created(fx.p("x.zip"))]);
}

#[test]
fn a_later_save_of_the_downloaded_file_is_not_a_second_download() {
    let mut fx = Fx::new("fs.download-finished", json!({}), &[]);
    fx.put("x.zip.part");
    fx.raw(1000, RawKind::Create, &["x.zip.part"]);
    fx.delete("x.zip.part");
    fx.put("x.zip");
    fx.raw(2000, RawKind::Name, &["x.zip"]);
    assert_eq!(fx.tick(2600).len(), 2);
    fx.raw(4000, RawKind::Modify, &["x.zip"]);
    assert_eq!(fx.tick(4600), vec![Obs::Saved(fx.p("x.zip"))]);
}

#[test]
fn only_the_download_kind_fires_download_finished() {
    let mut fx = Fx::new("fs.file-created", json!({}), &[]);
    fx.put("x.zip.part");
    fx.raw(1000, RawKind::Create, &["x.zip.part"]);
    fx.delete("x.zip.part");
    fx.put("x.zip");
    fx.raw(2000, RawKind::Name, &["x.zip"]);
    let out = fx.step(Signal::Tick { now: 2600 });
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].0, "fs.file-created");
}
