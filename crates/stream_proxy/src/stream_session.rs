//! What the proxy knows about one open stream.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, PoisonError};

use crate::entry_kind::EntryKind;
use crate::header_policy::HeaderPolicy;
use crate::stream_spec::StreamSpec;
use crate::url_table::{Entry, UrlTable};

/// The number the stream's own playlist always has.
pub(crate) const FIRST_PLAYLIST_ID: usize = 0;

/// One stream the player was given: its secret token, its headers and the addresses its playlists
/// have named so far.
#[derive(Debug)]
pub(crate) struct StreamSession {
    token: String,
    revoked: AtomicBool,
    headers: HeaderPolicy,
    table: Mutex<UrlTable>,
}

impl StreamSession {
    /// Starts a session whose first address (number 0) is the stream's own playlist.
    pub(crate) fn new(token: String, spec: &StreamSpec) -> Self {
        let mut table = UrlTable::default();
        table.register(&spec.url, EntryKind::Playlist);
        Self {
            token,
            revoked: AtomicBool::new(false),
            headers: HeaderPolicy::new(spec),
            table: Mutex::new(table),
        }
    }

    pub(crate) fn token(&self) -> &str {
        &self.token
    }

    pub(crate) fn headers(&self) -> &HeaderPolicy {
        &self.headers
    }

    /// True once the stream was closed or replaced; work in progress for it should stop.
    pub(crate) fn is_revoked(&self) -> bool {
        self.revoked.load(Ordering::SeqCst)
    }

    pub(crate) fn revoke(&self) {
        self.revoked.store(true, Ordering::SeqCst);
    }

    /// Numbers an address the stream's playlists named; `None` when the stream has too many.
    pub(crate) fn register(&self, url: &str, kind: EntryKind) -> Option<usize> {
        self.table
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .register(url, kind)
    }

    /// The address with number `id`.
    pub(crate) fn entry(&self, id: usize) -> Option<Entry> {
        self.table
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(id)
            .cloned()
    }
}
