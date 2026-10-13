//! Helpers shared by the hub's tests.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use trigger_core::testkit::FakeSource;
use trigger_core::{Availability, Handle, KindInfo, Rule, RuleError, Sink, Source, SourceError};

/// Wraps a `FakeSource` in an `Arc`, so a test keeps a handle on it after the
/// hub has taken ownership of the box. Also:
/// - `reject_bad`: `check` fails for any rule whose `when` has a key `bad`,
///   with the message `field bad: not allowed`.
/// - `stops`: counts how many times a `Handle` this source returned was dropped.
pub struct Shared {
    pub inner: Arc<FakeSource>,
    pub reject_bad: bool,
    pub stops: Arc<AtomicUsize>,
}

/// Returns the box for `Hub::new`, plus the two things a test reads afterwards.
pub fn shared(
    fake: FakeSource,
    reject_bad: bool,
) -> (Box<dyn Source>, Arc<FakeSource>, Arc<AtomicUsize>) {
    let inner = Arc::new(fake);
    let stops = Arc::new(AtomicUsize::new(0));
    let source = Shared {
        inner: Arc::clone(&inner),
        reject_bad,
        stops: Arc::clone(&stops),
    };
    (Box::new(source), inner, stops)
}

impl Source for Shared {
    fn id(&self) -> &'static str {
        self.inner.id()
    }

    fn kinds(&self) -> &'static [KindInfo] {
        self.inner.kinds()
    }

    fn availability(&self) -> Availability {
        self.inner.availability()
    }

    fn check(&self, rule: &Rule) -> Result<(), RuleError> {
        if self.reject_bad && rule.when.contains_key("bad") {
            return Err(RuleError::Field {
                field: "bad".to_string(),
                problem: "not allowed".to_string(),
            });
        }
        self.inner.check(rule)
    }

    fn start(&self, rules: Vec<Rule>, sink: Sink) -> Result<Handle, SourceError> {
        let handle = self.inner.start(rules, sink)?;
        let stops = Arc::clone(&self.stops);
        Ok(Handle::new(move || {
            drop(handle);
            stops.fetch_add(1, Ordering::SeqCst);
        }))
    }
}
