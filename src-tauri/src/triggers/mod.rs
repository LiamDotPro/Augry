//! Process triggers in the app: the local process definitions, the rules
//! worked out from them, and the glue to `trigger-hub`. Design:
//! plans/001-trigger-core/epic.md Decisions 10 to 16. Later cards add the
//! rest of the module; each adds its own `pub mod` line.

pub mod store;
pub mod variables;
