//! The only file that talks to the operating system. Slice C of the epic
//! replaces this stub with `find_engine`, the version check and the real
//! engine; everything else is in `logic.rs`.

// Removed when lib.rs uses everything in this file.
#![allow(dead_code)]

use trigger_core::Availability;

pub fn availability() -> Availability {
    Availability::Unsupported {
        reason: "not implemented yet".into(),
    }
}
