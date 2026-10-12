//! The only file that talks to the operating system: finds `gh` and runs it.
//! Slice C fills it in.

use trigger_core::Availability;

pub fn availability() -> Availability {
    Availability::Unsupported {
        reason: "not implemented yet".into(),
    }
}
