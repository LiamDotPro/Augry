//! The only file that talks to the operating system. The clock loop needs no
//! OS calls of its own (it reads the wall clock through `trigger_core`'s
//! `Clock`), so for now this only reports availability. Later cards add the
//! feed fetcher and the local zone name here.

use trigger_core::Availability;

pub fn availability() -> Availability {
    Availability::Available
}
