//! The hub: owns the trigger sources, applies a rule set to them, cools down
//! noisy rules and passes fired events to subscribers. See
//! docs/process-triggers.md Part 2.

pub mod cool_down;

use cool_down::CoolDown;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::Duration;
use trigger_core::{Availability, Event, KindInfo, Rule, RuleError, Source};

#[cfg(test)]
mod tests;

#[cfg(test)]
// Some helpers are used only by later tests.
#[allow(dead_code)]
mod test_support;

const DEFAULT_COOL_DOWN: Duration = Duration::from_secs(2);

/// How often the drain thread looks at the stop flag when nothing arrives.
const POLL: Duration = Duration::from_millis(50);

/// Owns the sources. Every method takes `&self`, so the app can keep one in
/// Tauri state.
pub struct Hub {
    sources: Vec<Box<dyn Source>>,
    /// Sources get clones of this as their sink, and `publish` sends into it.
    tx: Sender<Event>,
    subscribers: Arc<Mutex<Vec<Sender<Fired>>>>,
    cool_down: Arc<Mutex<CoolDown>>,
    stop: Arc<AtomicBool>,
    /// `None` only if the operating system refused to start the thread.
    thread: Option<JoinHandle<()>>,
}

/// One source as the frontend sees it.
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceInfo {
    pub id: &'static str,
    pub kinds: Vec<KindInfo>,
    pub availability: Availability,
}

/// An event that passed the cool down, as sent to subscribers.
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Fired {
    pub event: Event,
    /// How many events for the same rule were dropped by the cool down since
    /// the last `Fired` for that rule.
    pub coalesced: u32,
}

/// What happened to one rule in `Hub::apply`.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum RuleState {
    Active,
    Rejected { reason: String },
    SourceFailed { reason: String },
}

/// A poisoned lock only means another thread panicked; the data is still fine.
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Hub {
    pub fn new(sources: Vec<Box<dyn Source>>) -> Hub {
        let (tx, rx) = channel();
        let subscribers = Arc::new(Mutex::new(Vec::new()));
        let cool_down = Arc::new(Mutex::new(CoolDown::new(DEFAULT_COOL_DOWN)));
        let stop = Arc::new(AtomicBool::new(false));
        let thread = {
            let subscribers = Arc::clone(&subscribers);
            let cool_down = Arc::clone(&cool_down);
            let stop = Arc::clone(&stop);
            std::thread::Builder::new()
                .name("trigger-hub".to_string())
                .spawn(move || drain(rx, subscribers, cool_down, stop))
                .ok()
        };
        Hub {
            sources,
            tx,
            subscribers,
            cool_down,
            stop,
            thread,
        }
    }

    /// Replaces the default two second cool down per rule.
    pub fn with_cool_down(self, d: Duration) -> Hub {
        lock(&self.cool_down).set_window(d);
        self
    }

    /// Every source with its kinds and whether it can run here.
    pub fn catalogue(&self) -> Vec<SourceInfo> {
        self.sources
            .iter()
            .map(|s| SourceInfo {
                id: s.id(),
                kinds: s.kinds().to_vec(),
                availability: s.availability(),
            })
            .collect()
    }

    /// The index of the source that owns this kind. A kind belongs to the
    /// source whose id is the part before the first dot: `fs.file-saved`
    /// belongs to `fs`.
    fn source_index(&self, kind: &str) -> Option<usize> {
        let prefix = kind.split('.').next().unwrap_or("");
        self.sources.iter().position(|s| s.id() == prefix)
    }

    /// Asks the source that owns the rule's kind to check the rule. A kind no
    /// source owns is `RuleError::UnknownKind`.
    pub fn check(&self, rule: &Rule) -> Result<(), RuleError> {
        match self.source_index(&rule.kind) {
            Some(index) => self.sources[index].check(rule),
            None => Err(RuleError::UnknownKind(rule.kind.clone())),
        }
    }

    /// Sends an event into the same channel the sources use, so it goes through
    /// the cool down like any other. For triggers that come from inside the app.
    pub fn publish(&self, event: Event) {
        let _ = self.tx.send(event);
    }

    /// A new receiver of every `Fired` from now on. Drop it to unsubscribe.
    pub fn subscribe(&self) -> Receiver<Fired> {
        let (tx, rx) = channel();
        lock(&self.subscribers).push(tx);
        rx
    }
}

impl Drop for Hub {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// The body of the `trigger-hub` thread.
fn drain(
    rx: Receiver<Event>,
    subscribers: Arc<Mutex<Vec<Sender<Fired>>>>,
    cool_down: Arc<Mutex<CoolDown>>,
    stop: Arc<AtomicBool>,
) {
    while !stop.load(Ordering::SeqCst) {
        match rx.recv_timeout(POLL) {
            Ok(event) => {
                let admitted = lock(&cool_down).admit(&event.rule, event.at);
                if let Some(coalesced) = admitted {
                    let fired = Fired { event, coalesced };
                    // A subscriber whose receiver was dropped fails to send and is removed.
                    lock(&subscribers).retain(|s| s.send(fired.clone()).is_ok());
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
}
