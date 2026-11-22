//! Native verification browser, kept separate from the Slint window.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::path::Path;
use std::rc::Rc;

use tao::event_loop::{EventLoopProxy, EventLoopWindowTarget};
use tao::window::{Window, WindowBuilder};
use url::Url;

use crate::helper_events::HelperEvent;
use crate::verification::{BrowserEvent, BrowserIdentity, Cookies};

#[cfg(target_os = "linux")]
#[path = "webkit_browser.rs"]
mod platform;
#[cfg(windows)]
#[path = "webview2_browser.rs"]
mod platform;

pub(crate) use platform::{BrowserContext, BrowserView};

#[derive(Clone)]
pub(crate) struct BrowserSource {
    pub(super) target: Url,
    id: String,
    proxy: EventLoopProxy<HelperEvent>,
    events: Rc<RefCell<VecDeque<BrowserEvent>>>,
}

impl BrowserSource {
    pub(crate) fn new(target: Url, id: String, proxy: EventLoopProxy<HelperEvent>) -> Self {
        Self {
            target,
            id,
            proxy,
            events: Rc::default(),
        }
    }

    pub(crate) fn send(&self, event: BrowserEvent) {
        self.events.borrow_mut().push_back(event);
        if self
            .proxy
            .send_event(HelperEvent::Browser {
                id: self.id.clone(),
            })
            .is_err()
        {
            tracing::debug!(id = %self.id, "the verification event loop has closed");
        }
    }

    pub(crate) fn take_event(&self) -> Option<BrowserEvent> {
        self.events.borrow_mut().pop_front()
    }

    pub(super) fn unavailable(&self, operation: &str, error: impl std::fmt::Display) {
        tracing::warn!(id = %self.id, %operation, %error, "verification browser operation failed");
        self.send(BrowserEvent::Unavailable(operation.to_owned()));
    }

    pub(super) fn identity(&self, agent: String, cookies: Cookies) {
        let agent = cloudflare_window::script_text(&agent);
        let result = if agent.is_empty() {
            Err("the browser did not report its identity".to_owned())
        } else {
            Ok(BrowserIdentity { agent, cookies })
        };
        self.send(BrowserEvent::Identity(result));
    }
}

pub(crate) fn open_view(
    target: &EventLoopWindowTarget<HelperEvent>,
    context: &mut BrowserContext,
    host: &str,
    source: BrowserSource,
) -> Result<(BrowserView, Rc<Window>), String> {
    let window = Rc::new(
        WindowBuilder::new()
            .with_title(format!("Verify {host} — CloudStream"))
            .with_inner_size(tao::dpi::LogicalSize::new(960.0, 720.0))
            .with_focused(false)
            .with_visible(false)
            .build(target)
            .map_err(|error| format!("cannot open a verification window: {error}"))?,
    );
    let browser = BrowserView::open(context, &window, source)?;
    Ok((browser, window))
}

#[cfg(not(any(windows, target_os = "linux")))]
mod platform {
    use super::*;
    use crate::verification::DocumentId;

    pub(crate) struct BrowserContext;
    pub(crate) struct BrowserView;

    impl BrowserContext {
        pub(crate) fn new(_profile: &Path) -> Self {
            Self
        }
    }

    impl BrowserView {
        pub(crate) fn open(
            _context: &mut BrowserContext,
            _window: &Rc<Window>,
            _source: BrowserSource,
        ) -> Result<Self, String> {
            Err("interactive verification is unavailable on this platform".to_owned())
        }
        pub(crate) fn identify(&self) {}
        pub(crate) fn navigate(&self, _url: &Url) {}
        pub(crate) fn reveal(&self, _document: DocumentId) -> bool {
            false
        }
        pub(crate) fn is_current(&self, _document: DocumentId) -> bool {
            false
        }
        pub(crate) fn evaluate(
            &self,
            _script: &str,
            _done: impl Fn(String) + Send + 'static,
        ) -> Result<(), String> {
            Err("interactive verification is unavailable on this platform".to_owned())
        }
        pub(crate) fn resize(&self, _width: u32, _height: u32) {}
    }
}
