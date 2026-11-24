//! Everything a Slint callback needs, bundled so each handler takes one argument.

use std::cell::RefCell;
use std::rc::Rc;

use app_services::UiServices;

use crate::background_work::{EventPump, UiSender};
use crate::cloudflare_prompts::PromptQueue;
use crate::request_tickets::{RequestKind, Ticket};
use crate::CloudStream;

use super::app_extras::AppExtras;
use super::gl_video::GlVideoState;
use super::image_store::SharedImageStore;
use super::session::Session;

/// Shared handles of the UI thread. Cloning is cheap (reference counts only).
#[derive(Clone)]
pub(crate) struct BridgeContext {
    pub(crate) ui: slint::Weak<CloudStream>,
    pub(crate) services: Rc<UiServices>,
    pub(crate) session: Rc<RefCell<Session>>,
    pub(crate) pump: Rc<EventPump>,
    pub(crate) extras: Rc<RefCell<AppExtras>>,
    /// The Cloudflare checks the banner shows, one at a time.
    pub(crate) prompts: Rc<RefCell<PromptQueue>>,
    /// The decoded posters on screen, one picture per address.
    pub(crate) images: SharedImageStore,
    /// Zero-copy video: what the rendering notifier and the frame timer share.
    pub(crate) gl: Rc<GlVideoState>,
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

    /// A ticket for a new request of `kind`; earlier answers of that kind are dropped from now on.
    pub(crate) fn issue_ticket(&self, kind: RequestKind) -> Ticket {
        self.session.borrow_mut().tickets.issue(kind)
    }
}
