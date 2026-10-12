//! Fires on container and image events from Docker and Podman. See README.md.
//!
//! Glue only: the kinds this crate fires, the `Source` implementation, and the
//! thread that feeds engine events through the pure logic into the sink. Rules
//! live in `rule.rs`, decisions in `logic.rs`, the OS in `os.rs`.

mod logic;
mod os;
mod plan;
mod rule;
mod supervisor;

use trigger_core::{
    Availability, FieldInfo, Handle, KindInfo, Platforms, Rule, RuleError, Sink, Source,
    SourceError,
};

const WHEN: &[FieldInfo] = &[
    FieldInfo {
        name: "name",
        doc: "Glob on the container name, or on the image reference for image-built",
        required: false,
    },
    FieldInfo {
        name: "image",
        doc: "Glob on the image. docker.io/library/ and :latest are optional",
        required: false,
    },
    FieldInfo {
        name: "label",
        doc: "A label the container or image must have, as key=value",
        required: false,
    },
    FieldInfo {
        name: "engine",
        doc: "docker, podman or auto. Default auto",
        required: false,
    },
];

const ENGINE: FieldInfo = FieldInfo {
    name: "engine",
    doc: "docker or podman",
    required: true,
};
const CONTAINER_ID: FieldInfo = FieldInfo {
    name: "container_id",
    doc: "First 12 characters of the container id",
    required: true,
};
const NAME: FieldInfo = FieldInfo {
    name: "name",
    doc: "Container name, or the image reference for image-built",
    required: true,
};
const IMAGE: FieldInfo = FieldInfo {
    name: "image",
    doc: "Image the container runs, as the engine reports it",
    required: true,
};
const TIME: FieldInfo = FieldInfo {
    name: "time",
    doc: "When the engine reported it, in milliseconds since 1970",
    required: true,
};
const EXIT_CODE: FieldInfo = FieldInfo {
    name: "exit_code",
    doc: "Exit code of the main process, -1 when the engine did not report one",
    required: true,
};
const HEALTH: FieldInfo = FieldInfo {
    name: "health",
    doc: "healthy or unhealthy",
    required: true,
};

const PAYLOAD: &[FieldInfo] = &[ENGINE, CONTAINER_ID, NAME, IMAGE, TIME];
const PAYLOAD_STOPPED: &[FieldInfo] = &[ENGINE, CONTAINER_ID, NAME, IMAGE, TIME, EXIT_CODE];
const PAYLOAD_HEALTH: &[FieldInfo] = &[ENGINE, CONTAINER_ID, NAME, IMAGE, TIME, HEALTH];
const PAYLOAD_IMAGE: &[FieldInfo] = &[ENGINE, NAME, IMAGE, TIME];

/// Every kind this crate fires. The ids are a contract with saved process
/// definitions: add kinds, never rename one.
pub static KINDS: &[KindInfo] = &[
    KindInfo {
        id: "docker.container-started",
        title: "Container started",
        summary: "A container started.",
        platforms: Platforms::ALL,
        permission: None,
        when: WHEN,
        payload: PAYLOAD,
    },
    KindInfo {
        id: "docker.container-stopped",
        title: "Container stopped",
        summary: "A container's main process ended, by itself or because it was stopped.",
        platforms: Platforms::ALL,
        permission: None,
        when: WHEN,
        payload: PAYLOAD_STOPPED,
    },
    KindInfo {
        id: "docker.container-healthy",
        title: "Container healthy",
        summary: "A container's health check started passing.",
        platforms: Platforms::ALL,
        permission: None,
        when: WHEN,
        payload: PAYLOAD_HEALTH,
    },
    KindInfo {
        id: "docker.container-unhealthy",
        title: "Container unhealthy",
        summary: "A container's health check started failing.",
        platforms: Platforms::ALL,
        permission: None,
        when: WHEN,
        payload: PAYLOAD_HEALTH,
    },
    KindInfo {
        id: "docker.image-built",
        title: "Image built",
        summary: "An image was built and tagged. A manual tag also counts.",
        platforms: Platforms::ALL,
        permission: None,
        when: WHEN,
        payload: PAYLOAD_IMAGE,
    },
];

pub struct DockerSource;

impl Source for DockerSource {
    fn id(&self) -> &'static str {
        "docker"
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
                rule::parse(r).map(|when| logic::Armed {
                    id: r.id.clone(),
                    kind: r.kind.clone(),
                    when,
                })
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| SourceError::Os(e.to_string()))?;
        // The engine child process and the thread are added by the glue card
        // in slice C.
        let _ = sink;
        Err(SourceError::Unavailable("not implemented yet".into()))
    }
}
