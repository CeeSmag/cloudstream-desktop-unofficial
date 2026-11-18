//! Everything a Slint callback needs, bundled so each handler takes one argument.

use std::cell::RefCell;
use std::rc::Rc;

use app_services::UiServices;

use crate::background_work::{EventPump, UiSender};
use crate::poster_images::PosterImageCache;
use crate::CloudStream;

use super::session::{AppExtras, Session};

/// Shared handles of the UI thread. Cloning is cheap (reference counts only).
#[derive(Clone)]
pub(crate) struct BridgeContext {
    pub(crate) ui: slint::Weak<CloudStream>,
    pub(crate) services: Rc<UiServices>,
    pub(crate) session: Rc<RefCell<Session>>,
    pub(crate) pump: Rc<EventPump>,
    pub(crate) extras: Rc<RefCell<AppExtras>>,
    pub(crate) posters: PosterImageCache,
}

impl BridgeContext {
    /// The window, unless it was closed.
    pub(crate) fn window(&self) -> Option<CloudStream> {
        self.ui.upgrade()
    }

    /// A sender for worker threads.
    pub(crate) fn sender(&self) -> UiSender {
        self.pump.sender()
    }

    /// Services a worker thread can own.
    pub(crate) fn owned_services(&self) -> UiServices {
        (*self.services).clone()
    }

    /// The exchange folder shared with the sidecar's Cloudflare solver.
    pub(crate) fn cloudflare_dir(&self) -> Option<std::path::PathBuf> {
        self.extras
            .borrow()
            .data_dir
            .as_ref()
            .map(|dir| dir.join("sidecar").join("cloudflare"))
    }
}
