//! The only file that talks to the operating system. Slice C adds the watcher
//! and the folder scan here; for now it only answers `availability`.

use trigger_core::Availability;

pub fn availability() -> Availability {
    Availability::Unsupported {
        reason: "not implemented yet".into(),
    }
}
