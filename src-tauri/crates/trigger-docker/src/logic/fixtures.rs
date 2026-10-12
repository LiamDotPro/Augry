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
