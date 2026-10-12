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
