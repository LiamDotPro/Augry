//! A stand in for `gh` that serves saved answers and remembers what it was
//! asked. Compiled for tests only.

use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard};

use super::cli::Gh;
use super::response::GhResponse;

#[derive(Default)]
struct Inner {
    me: Option<String>,
    logins: usize,
    answers: BTreeMap<String, VecDeque<GhResponse>>,
    calls: Vec<(String, Option<String>)>,
}

/// Clones share their state, so a test keeps one and gives the poller another.
#[derive(Clone, Default)]
pub struct FakeGh {
    inner: Arc<Mutex<Inner>>,
}

impl FakeGh {
    /// `me` is what `login()` answers; None means `gh` cannot say.
    pub fn new(me: Option<&str>) -> FakeGh {
        let gh = FakeGh::default();
        gh.set_me(me);
        gh
    }

    pub fn set_me(&self, me: Option<&str>) {
        self.lock().me = me.map(str::to_string);
    }

    /// Queues an answer for a request path. Answers for one path are served in
    /// the order queued, and the last one is served again and again. A path
    /// nothing was queued for gets a 404.
    pub fn serve(&self, path: &str, response: GhResponse) {
        self.lock()
            .answers
            .entry(path.to_string())
            .or_default()
            .push_back(response);
    }

    /// Every request so far: the path and the `If-None-Match` it carried.
    pub fn calls(&self) -> Vec<(String, Option<String>)> {
        self.lock().calls.clone()
    }

    /// Just the paths of `calls`.
    pub fn paths(&self) -> Vec<String> {
        self.lock().calls.iter().map(|(p, _)| p.clone()).collect()
    }

    /// How many times `login()` was asked.
    pub fn logins(&self) -> usize {
        self.lock().logins
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl Gh for FakeGh {
    fn get(&self, path: &str, etag: Option<&str>) -> GhResponse {
        let mut inner = self.lock();
        inner
            .calls
            .push((path.to_string(), etag.map(str::to_string)));
        match inner.answers.get_mut(path) {
            Some(queue) if queue.len() > 1 => queue.pop_front().unwrap(),
            Some(queue) => queue.front().cloned().unwrap(),
            None => http(404, &[], "{\"message\":\"Not Found\"}"),
        }
    }

    fn login(&self) -> Option<String> {
        let mut inner = self.lock();
        inner.logins += 1;
        inner.me.clone()
    }
}

/// A 200 with an `ETag`.
pub fn ok(body: &str, etag: &str) -> GhResponse {
    ok_with(body, etag, &[])
}

/// A 200 with an `ETag` and more headers (names in lower case, as the parser
/// gives them).
pub fn ok_with(body: &str, etag: &str, headers: &[(&str, &str)]) -> GhResponse {
    let mut all = vec![("etag".to_string(), etag.to_string())];
    all.extend(headers.iter().map(|(n, v)| (n.to_string(), v.to_string())));
    GhResponse::Ok {
        status: 200,
        etag: Some(etag.to_string()),
        headers: all,
        body: body.to_string(),
    }
}

pub fn not_modified() -> GhResponse {
    GhResponse::NotModified { headers: vec![] }
}

pub fn http(status: u16, headers: &[(&str, &str)], body: &str) -> GhResponse {
    GhResponse::Http {
        status,
        headers: headers
            .iter()
            .map(|(n, v)| (n.to_string(), v.to_string()))
            .collect(),
        body: body.to_string(),
    }
}

pub fn transport(message: &str) -> GhResponse {
    GhResponse::Transport(message.to_string())
}
