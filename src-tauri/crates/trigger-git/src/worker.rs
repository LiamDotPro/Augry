//! The thread that joins the OS layer to the pure logic. It owns every
//! repository slot, waits for file events and for the polled kinds' times,
//! and sends events to the sink. Glue only: the decisions are in `logic`.

// Removed when lib.rs reads everything in this file.
#![allow(dead_code)]

mod repo;

// The allow is removed when lib.rs uses these.
#[allow(unused_imports)]
pub use repo::{Entry, RepoSlot};
