//! Fires when files change under a watched folder. See README.md.
//!
//! Glue only: the kinds this crate fires, the `Source` implementation, and the
//! thread that feeds OS signals through the pure logic into the sink. Rules
//! live in `rule.rs`, decisions in `logic.rs`, the OS in `os.rs`.

mod logic;
mod os;
mod rule;

use trigger_core::{
    Availability, FieldInfo, Handle, KindInfo, Platforms, Rule, RuleError, Sink, Source,
    SourceError,
};

const WHEN: &[FieldInfo] = &[
    FieldInfo {
        name: "path",
        doc: "Folder to watch, absolute. Everything below it is watched.",
        required: true,
    },
    FieldInfo {
        name: "glob",
        doc: "Which files count, matched against the path below the folder. Default **/*",
        required: false,
    },
    FieldInfo {
        name: "ignore",
        doc: "Globs to skip, added to the built in ones (.git, node_modules, target, temp and partial download files)",
        required: false,
    },
    FieldInfo {
        name: "settle_ms",
        doc: "Milliseconds a file must be quiet before it counts as changed. Default 500",
        required: false,
    },
];

const PAYLOAD: &[FieldInfo] = &[
    FieldInfo {
        name: "path",
        doc: "Absolute path of the file",
        required: true,
    },
    FieldInfo {
        name: "name",
        doc: "File name",
        required: true,
    },
    FieldInfo {
        name: "relative",
        doc: "Path below the watched folder, with / separators",
        required: true,
    },
    FieldInfo {
        name: "ext",
        doc: "Extension without the dot, empty when there is none",
        required: true,
    },
];

const PAYLOAD_RENAMED: &[FieldInfo] = &[
    FieldInfo {
        name: "path",
        doc: "Absolute path of the file after the rename",
        required: true,
    },
    FieldInfo {
        name: "name",
        doc: "File name after the rename",
        required: true,
    },
    FieldInfo {
        name: "relative",
        doc: "Path below the watched folder, with / separators",
        required: true,
    },
    FieldInfo {
        name: "ext",
        doc: "Extension without the dot, empty when there is none",
        required: true,
    },
    FieldInfo {
        name: "from",
        doc: "Absolute path before the rename",
        required: true,
    },
];

/// Every kind this crate fires. The ids are a contract with saved process
/// definitions: add kinds, never rename one.
pub static KINDS: &[KindInfo] = &[
    KindInfo {
        id: "fs.file-saved",
        title: "File saved",
        summary: "A file that already existed was written to.",
        platforms: Platforms::ALL,
        permission: None,
        when: WHEN,
        payload: PAYLOAD,
    },
    KindInfo {
        id: "fs.file-created",
        title: "File created",
        summary: "A new file appeared.",
        platforms: Platforms::ALL,
        permission: None,
        when: WHEN,
        payload: PAYLOAD,
    },
    KindInfo {
        id: "fs.file-removed",
        title: "File removed",
        summary: "A file was deleted.",
        platforms: Platforms::ALL,
        permission: None,
        when: WHEN,
        payload: PAYLOAD,
    },
    KindInfo {
        id: "fs.file-renamed",
        title: "File renamed",
        summary: "A file was renamed or moved inside the watched folder.",
        platforms: Platforms::ALL,
        permission: None,
        when: WHEN,
        payload: PAYLOAD_RENAMED,
    },
    KindInfo {
        id: "fs.download-finished",
        title: "Download finished",
        summary: "A browser finished a download and the file got its final name.",
        platforms: Platforms::ALL,
        permission: None,
        when: WHEN,
        payload: PAYLOAD,
    },
];

pub struct FsSource;

impl Source for FsSource {
    fn id(&self) -> &'static str {
        "fs"
    }

    fn kinds(&self) -> &'static [KindInfo] {
        KINDS
    }

    fn availability(&self) -> Availability {
        os::availability()
    }

    fn check(&self, rule: &Rule) -> Result<(), RuleError> {
        rule::parse(rule).map(|_| ())
    }

    fn start(&self, rules: Vec<Rule>, sink: Sink) -> Result<Handle, SourceError> {
        let _armed = rules
            .iter()
            .map(|r| {
                rule::parse(r).map(|p| logic::Armed {
                    id: r.id.clone(),
                    kind: r.kind.clone(),
                    root: p.when.path.clone(),
                    when: p.when,
                    glob: p.glob,
                    ignore: p.ignore,
                })
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| SourceError::Os(e.to_string()))?;
        // The watcher and the thread are added by the glue card (T27).
        let _ = sink;
        Err(SourceError::Unavailable("not implemented yet".into()))
    }
}
