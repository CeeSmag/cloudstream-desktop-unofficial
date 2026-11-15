//! Which streams the proxy currently serves: the newest and, until the next one opens, the one it
//! replaced.
//!
//! The replaced stream stays valid on purpose. When the viewer switches source the player is still
//! reading the old one until it has accepted the new address; cutting the old stream first would
//! make that read fail, and a failed read looks like a broken source and starts the fallback.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use crate::proxy_error::ProxyError;
use crate::session_token::new_token;
use crate::stream_session::StreamSession;
use crate::stream_spec::StreamSpec;
use crate::url_parts::is_http;

#[derive(Debug, Default)]
struct Slots {
    current: Option<Arc<StreamSession>>,
    previous: Option<Arc<StreamSession>>,
}

/// The open streams. Opening, closing and looking up are each one step under one lock.
#[derive(Debug, Default)]
pub(crate) struct SessionRegistry {
    slots: Mutex<Slots>,
    opened: AtomicU64,
}

impl SessionRegistry {
    fn lock(&self) -> std::sync::MutexGuard<'_, Slots> {
        self.slots.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Opens a stream: it becomes the newest, the newest becomes the replaced one, and the one
    /// replaced before is revoked.
    pub(crate) fn open(&self, spec: &StreamSpec) -> Result<Arc<StreamSession>, ProxyError> {
        if !is_http(&spec.url) {
            return Err(ProxyError::UnsupportedAddress);
        }
        let counter = self.opened.fetch_add(1, Ordering::Relaxed);
        let session = Arc::new(StreamSession::new(new_token(counter), spec));
        let mut slots = self.lock();
        if let Some(oldest) = slots.previous.take() {
            oldest.revoke();
        }
        slots.previous = slots.current.replace(Arc::clone(&session));
        Ok(session)
    }

    /// The live stream with this token, if there is one.
    pub(crate) fn lookup(&self, token: &str) -> Option<Arc<StreamSession>> {
        let slots = self.lock();
        let found = [&slots.current, &slots.previous]
            .into_iter()
            .flatten()
            .find(|session| session.token() == token)
            .cloned();
        found
    }

    /// Revokes every stream; what they are still sending stops at the next chunk.
    pub(crate) fn close(&self) {
        let mut slots = self.lock();
        for session in [slots.current.take(), slots.previous.take()]
            .into_iter()
            .flatten()
        {
            session.revoke();
        }
    }
}

#[cfg(test)]
#[path = "session_registry_tests.rs"]
mod tests;
