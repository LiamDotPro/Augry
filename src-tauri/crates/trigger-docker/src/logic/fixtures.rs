//! Lines the engines print, used by the tests in `tests.rs`.
//!
//! The Docker lines marked "captured" are verbatim output of
//! `docker events --format '{{json .}}'` from Docker Desktop, Engine 29.4.3
//! (research note R01). Lines marked "hand made" follow the same shape but
//! were written by hand. Nothing here comes from a user's machine.

/// Captured: `docker run` of an image with a health check.
pub const DOCKER_START: &str = r#"{"Type":"container","Action":"start","Actor":{"ID":"ef63764e96795c073e6f76c2d2512d8f3b52874bea6b51f94d0c166088379afe","Attributes":{"com.example.k":"v","com.example.team":"x","desktop.docker.io/ports.scheme":"v2","image":"r01test:1","name":"r01c"}},"scope":"local","time":1791837794,"timeNano":1791837794607003258}"#;

/// Captured.
pub const DOCKER_UNHEALTHY: &str = r#"{"Type":"container","Action":"health_status: unhealthy","Actor":{"ID":"ef63764e96795c073e6f76c2d2512d8f3b52874bea6b51f94d0c166088379afe","Attributes":{"com.example.k":"v","com.example.team":"x","desktop.docker.io/ports.scheme":"v2","image":"r01test:1","name":"r01c"}},"scope":"local","time":1791837795,"timeNano":1791837795647713133}"#;

/// Captured.
pub const DOCKER_HEALTHY: &str = r#"{"Type":"container","Action":"health_status: healthy","Actor":{"ID":"ef63764e96795c073e6f76c2d2512d8f3b52874bea6b51f94d0c166088379afe","Attributes":{"com.example.k":"v","com.example.team":"x","desktop.docker.io/ports.scheme":"v2","image":"r01test:1","name":"r01c"}},"scope":"local","time":1791837797,"timeNano":1791837797743493218}"#;

/// Captured: part of `docker stop -t 1`. Must be ignored.
pub const DOCKER_KILL: &str = r#"{"Type":"container","Action":"kill","Actor":{"ID":"ef63764e96795c073e6f76c2d2512d8f3b52874bea6b51f94d0c166088379afe","Attributes":{"com.example.k":"v","com.example.team":"x","desktop.docker.io/ports.scheme":"v2","image":"r01test:1","name":"r01c","signal":"9"}},"scope":"local","time":1791837807,"timeNano":1791837807655361458}"#;

/// Captured: part of `docker stop`. Must be ignored, `die` is the event used.
pub const DOCKER_STOP: &str = r#"{"Type":"container","Action":"stop","Actor":{"ID":"ef63764e96795c073e6f76c2d2512d8f3b52874bea6b51f94d0c166088379afe","Attributes":{"com.example.k":"v","com.example.team":"x","desktop.docker.io/ports.scheme":"v2","image":"r01test:1","name":"r01c"}},"scope":"local","time":1791837807,"timeNano":1791837807760686416}"#;

/// Captured: the container above ended after `docker stop`.
pub const DOCKER_DIE_137: &str = r#"{"Type":"container","Action":"die","Actor":{"ID":"ef63764e96795c073e6f76c2d2512d8f3b52874bea6b51f94d0c166088379afe","Attributes":{"com.example.k":"v","com.example.team":"x","desktop.docker.io/ports.scheme":"v2","execDuration":"13","exitCode":"137","image":"r01test:1","name":"r01c"}},"scope":"local","time":1791837807,"timeNano":1791837807764166125}"#;

/// Captured: a container that exited by itself with code 3 (no `stop` event).
pub const DOCKER_DIE_3: &str = r#"{"Type":"container","Action":"die","Actor":{"ID":"b2b0cf48d0c03b1e222b1270f8519ef55058954383be0ea0be1251e10ee22244","Attributes":{"desktop.docker.io/ports.scheme":"v2","execDuration":"0","exitCode":"3","image":"alpine:3","name":"r01x"}},"scope":"local","time":1791837807,"timeNano":1791837807993939750}"#;

/// Hand made, shaped like R01's description of `exec_die` (one per health
/// probe). Has its own exitCode and execID and must not count as stopped.
pub const DOCKER_EXEC_DIE: &str = r#"{"Type":"container","Action":"exec_die","Actor":{"ID":"ef63764e96795c073e6f76c2d2512d8f3b52874bea6b51f94d0c166088379afe","Attributes":{"com.example.k":"v","execID":"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef","exitCode":"1","image":"r01test:1","name":"r01c"}},"scope":"local","time":1791837795,"timeNano":1791837795100000000}"#;

/// Hand made. Must be ignored.
pub const DOCKER_EXEC_START: &str = r#"{"Type":"container","Action":"exec_start: test -f /ok","Actor":{"ID":"ef63764e96795c073e6f76c2d2512d8f3b52874bea6b51f94d0c166088379afe","Attributes":{"execID":"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef","image":"r01test:1","name":"r01c"}},"scope":"local","time":1791837795,"timeNano":1791837795090000000}"#;

/// Hand made. Must be ignored.
pub const DOCKER_CREATE: &str = r#"{"Type":"container","Action":"create","Actor":{"ID":"ef63764e96795c073e6f76c2d2512d8f3b52874bea6b51f94d0c166088379afe","Attributes":{"image":"r01test:1","name":"r01c"}},"scope":"local","time":1791837794,"timeNano":1791837794500000000}"#;

/// Hand made. Must be ignored.
pub const DOCKER_DESTROY: &str = r#"{"Type":"container","Action":"destroy","Actor":{"ID":"ef63764e96795c073e6f76c2d2512d8f3b52874bea6b51f94d0c166088379afe","Attributes":{"image":"r01test:1","name":"r01c"}},"scope":"local","time":1791837808,"timeNano":1791837808000000000}"#;

/// Hand made. Not a container event at all: must be ignored.
pub const DOCKER_NETWORK_CONNECT: &str = r#"{"Type":"network","Action":"connect","Actor":{"ID":"7c1e0f5a2b3d4c5e6f708192a3b4c5d6e7f8091a2b3c4d5e6f708192a3b4c5d6","Attributes":{"container":"ef63764e96795c073e6f76c2d2512d8f3b52874bea6b51f94d0c166088379afe","name":"bridge","type":"bridge"}},"scope":"local","time":1791837794,"timeNano":1791837794550000000}"#;

/// Hand made, a verdict R01 says may exist: `starting` must be ignored.
pub const DOCKER_HEALTH_STARTING: &str = r#"{"Type":"container","Action":"health_status: starting","Actor":{"ID":"ef63764e96795c073e6f76c2d2512d8f3b52874bea6b51f94d0c166088379afe","Attributes":{"image":"r01test:1","name":"r01c"}},"scope":"local","time":1791837794,"timeNano":1791837794700000000}"#;

/// Captured: BuildKit with the containerd image store, the first of the two
/// lines a whole build prints. Must be ignored (it also fires once per step
/// of a legacy build).
pub const DOCKER_IMAGE_CREATE: &str = r#"{"Type":"image","Action":"create","Actor":{"ID":"sha256:d760162063cb2959aa76446b17be2d95caa8289dd13aca56a65e12ec54eafa9f","Attributes":{"com.example.team":"x","name":"sha256:d760162063cb2959aa76446b17be2d95caa8289dd13aca56a65e12ec54eafa9f"}},"scope":"local","time":1791837793,"timeNano":1791837793998815716}"#;

/// Captured: the second line of that build, the one that means "built".
pub const DOCKER_IMAGE_TAG: &str = r#"{"Type":"image","Action":"tag","Actor":{"ID":"sha256:d760162063cb2959aa76446b17be2d95caa8289dd13aca56a65e12ec54eafa9f","Attributes":{"com.example.team":"x","name":"r01test:1"}},"scope":"local","time":1791837794,"timeNano":1791837794011737591}"#;

/// Hand made. Must be ignored.
pub const DOCKER_IMAGE_PULL: &str = r#"{"Type":"image","Action":"pull","Actor":{"ID":"docker.io/library/alpine:3","Attributes":{"name":"docker.io/library/alpine"}},"scope":"local","time":1791837700,"timeNano":1791837700000000000}"#;

/// Hand made. Must be ignored.
pub const DOCKER_IMAGE_UNTAG: &str = r#"{"Type":"image","Action":"untag","Actor":{"ID":"sha256:d760162063cb2959aa76446b17be2d95caa8289dd13aca56a65e12ec54eafa9f","Attributes":{"name":"sha256:d760162063cb2959aa76446b17be2d95caa8289dd13aca56a65e12ec54eafa9f"}},"scope":"local","time":1791837900,"timeNano":1791837900000000000}"#;

// Podman lines. SYNTHETIC: R01 had no Podman to capture from. They were made by
// marshalling Podman v5.6.0's own event struct, so key names and order are
// right for v5, but they are not captured from a running Podman. Replace them
// with a capture when a machine with Podman is available.

/// Synthetic.
pub const PODMAN_START: &str = r#"{"ID":"b2b0cf48d0c03b1e222b1270f8519ef55058954383be0ea0be1251e10ee22244","Image":"docker.io/library/alpine:3","Name":"r01c","Status":"start","time":1791799200,"timeNano":1791799200123456789,"Type":"container","Attributes":{"com.example.k":"v"}}"#;

/// Synthetic.
pub const PODMAN_DIED_3: &str = r#"{"ContainerExitCode":3,"ID":"b2b0cf48d0c03b1e222b1270f8519ef55058954383be0ea0be1251e10ee22244","Image":"docker.io/library/alpine:3","Name":"r01c","Status":"died","time":1791799200,"timeNano":1791799200123456789,"Type":"container","Attributes":{"com.example.k":"v"}}"#;

/// Synthetic.
pub const PODMAN_DIED_137: &str = r#"{"ContainerExitCode":137,"ID":"b2b0cf48d0c03b1e222b1270f8519ef55058954383be0ea0be1251e10ee22244","Image":"docker.io/library/alpine:3","Name":"r01c","Status":"died","time":1791799200,"timeNano":1791799200123456789,"Type":"container","Attributes":{"com.example.k":"v"}}"#;

/// Synthetic. Must be ignored.
pub const PODMAN_STOP: &str = r#"{"ID":"b2b0cf48d0c03b1e222b1270f8519ef55058954383be0ea0be1251e10ee22244","Image":"docker.io/library/alpine:3","Name":"r01c","Status":"stop","time":1791799200,"timeNano":1791799200123456789,"Type":"container","Attributes":{"com.example.k":"v"}}"#;

/// Synthetic.
pub const PODMAN_HEALTHY: &str = r#"{"ID":"b2b0cf48d0c03b1e222b1270f8519ef55058954383be0ea0be1251e10ee22244","Image":"docker.io/library/alpine:3","Name":"r01c","Status":"health_status","time":1791799200,"timeNano":1791799200123456789,"Type":"container","health_status":"healthy","Attributes":{"com.example.k":"v"}}"#;

/// Synthetic.
pub const PODMAN_UNHEALTHY: &str = r#"{"ID":"b2b0cf48d0c03b1e222b1270f8519ef55058954383be0ea0be1251e10ee22244","Image":"docker.io/library/alpine:3","Name":"r01c","Status":"health_status","time":1791799200,"timeNano":1791799200123456789,"Type":"container","health_status":"unhealthy","Attributes":{"com.example.k":"v"}}"#;

/// Synthetic. Podman's image build event: no id, `Attributes` is null.
pub const PODMAN_BUILD: &str = r#"{"Name":"localhost/r01test:1","Status":"build","time":1791799200,"timeNano":1791799200123456789,"Type":"image","Attributes":null}"#;

/// Hand made. Must be ignored.
pub const PODMAN_CREATE: &str = r#"{"ID":"b2b0cf48d0c03b1e222b1270f8519ef55058954383be0ea0be1251e10ee22244","Image":"docker.io/library/alpine:3","Name":"r01c","Status":"create","time":1791799200,"timeNano":1791799200123456789,"Type":"container","Attributes":{"com.example.k":"v"}}"#;

/// Hand made. Must be ignored: a pulled image is not a built one.
pub const PODMAN_IMAGE_PULL: &str = r#"{"ID":"sha256:d760162063cb2959aa76446b17be2d95caa8289dd13aca56a65e12ec54eafa9f","Name":"docker.io/library/alpine:3","Status":"pull","time":1791799200,"timeNano":1791799200123456789,"Type":"image","Attributes":null}"#;
