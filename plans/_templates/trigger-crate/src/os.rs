//! The only file that talks to the operating system. Each platform reports
//! `Signal`s into the channel until `stop` is set; everything else is in
//! `logic.rs`. A platform with more than a few lines gets its own file
//! (`os/macos.rs`) behind the same two functions.

use std::sync::atomic::AtomicBool;
use std::sync::mpsc::Sender;
use std::sync::Arc;

use trigger_core::{Availability, SourceError};

use crate::logic::{Armed, Signal};

pub fn availability() -> Availability {
    Availability::Unsupported { reason: "not implemented yet".into() }
}

/// Starts reporting signals for the armed rules. Returns at once; the work
/// happens on a named thread or an OS callback, and ends when `stop` is set.
pub fn listen(_armed: &[Armed], _stop: Arc<AtomicBool>, _tx: Sender<Signal>) -> Result<(), SourceError> {
    Err(SourceError::Unavailable("not implemented yet".into()))
}
