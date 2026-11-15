//! A scripted `Upstream` for tests: it answers from a function and remembers every request.

use std::io::Cursor;
use std::sync::{Mutex, PoisonError};

use crate::upstream::{Hop, HopResponse, Upstream, UpstreamError};

type Script = Box<dyn Fn(&Hop) -> Result<HopResponse, UpstreamError> + Send + Sync>;

pub(crate) struct FakeUpstream {
    script: Script,
    hops: Mutex<Vec<Hop>>,
}

impl FakeUpstream {
    pub(crate) fn new(
        script: impl Fn(&Hop) -> Result<HopResponse, UpstreamError> + Send + Sync + 'static,
    ) -> Self {
        Self {
            script: Box::new(script),
            hops: Mutex::new(Vec::new()),
        }
    }

    /// Every request made so far, in order.
    pub(crate) fn hops(&self) -> Vec<Hop> {
        self.hops
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl Upstream for FakeUpstream {
    fn get(&self, hop: &Hop) -> Result<HopResponse, UpstreamError> {
        self.hops
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(hop.clone());
        (self.script)(hop)
    }
}

/// An answer with a status, headers and a body.
pub(crate) fn answer(
    status: u16,
    headers: &[(&str, &str)],
    body: &[u8],
) -> Result<HopResponse, UpstreamError> {
    Ok(HopResponse {
        status,
        headers: headers
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect(),
        body: Box::new(Cursor::new(body.to_vec())),
    })
}

/// A request that failed on the way.
pub(crate) fn failure() -> Result<HopResponse, UpstreamError> {
    Err(UpstreamError::Request {
        host: "host.test".to_owned(),
        reason: "connection reset".to_owned(),
    })
}

/// A redirect to `location`.
pub(crate) fn redirect(location: &str) -> Result<HopResponse, UpstreamError> {
    answer(302, &[("Location", location)], b"")
}
