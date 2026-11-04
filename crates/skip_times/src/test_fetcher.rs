//! A fetcher for tests: answers from a script and remembers what it was asked.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use crate::skip_fetcher::{SkipError, SkipFetcher};

type Reply = Result<String, SkipError>;

/// Answers every URL with one reply, or a different one for URLs that start with a given text.
pub(crate) struct ScriptedFetcher {
    default: Reply,
    routes: Vec<(String, Reply)>,
    delay: Duration,
    asked: Mutex<Vec<String>>,
    calls: AtomicUsize,
}

impl ScriptedFetcher {
    fn with(default: Reply) -> Self {
        Self {
            default,
            routes: Vec::new(),
            delay: Duration::ZERO,
            asked: Mutex::new(Vec::new()),
            calls: AtomicUsize::new(0),
        }
    }

    pub(crate) fn answering(body: &str) -> Self {
        Self::with(Ok(body.to_owned()))
    }

    pub(crate) fn failing(error: SkipError) -> Self {
        Self::with(Err(error))
    }

    /// URLs starting with `prefix` get `reply` instead of the default.
    pub(crate) fn route(mut self, prefix: &str, reply: Reply) -> Self {
        self.routes.push((prefix.to_owned(), reply));
        self
    }

    /// Every answer takes this long, so concurrent callers overlap.
    pub(crate) fn slow(mut self, delay: Duration) -> Self {
        self.delay = delay;
        self
    }

    pub(crate) fn urls(&self) -> Vec<String> {
        self.asked
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    pub(crate) fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl SkipFetcher for ScriptedFetcher {
    fn get_text(&self, url: &str) -> Result<String, SkipError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.asked
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(url.to_owned());
        if !self.delay.is_zero() {
            std::thread::sleep(self.delay);
        }
        self.routes
            .iter()
            .find(|(prefix, _)| url.starts_with(prefix.as_str()))
            .map_or_else(|| self.default.clone(), |(_, reply)| reply.clone())
    }
}
