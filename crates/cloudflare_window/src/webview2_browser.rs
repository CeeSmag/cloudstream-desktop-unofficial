//! Windows browser hosting and native visibility control.

#[path = "webview2_protocol.rs"]
mod protocol;

use std::cell::{Cell, RefCell};
use std::rc::Weak;

use tao::platform::windows::WindowExtWindows;
use windows_webview::{EnvironmentOptions, WebViewHost};

use super::*;
use crate::browser_cookies::{checked, expiry_from_fractional_seconds};
use crate::verification::DocumentId;
use cloudflare_bridge::{CookieParts, CookieValue};

pub(crate) struct BrowserContext(EnvironmentOptions);

/// WebView2's browser is not replaced (see [`BrowserView::recycle`]), so recycling needs nothing.
pub(crate) struct RecycleParts;

impl RecycleParts {
    pub(crate) fn new(
        _context: &mut BrowserContext,
        _window: &Rc<Window>,
        _source: &BrowserSource,
    ) -> Self {
        Self
    }
}

impl BrowserContext {
    pub(crate) fn new(profile: &Path) -> Self {
        Self(EnvironmentOptions::new().user_data_folder(profile.to_string_lossy()))
    }
}

pub(crate) struct BrowserView {
    slot: Rc<RefCell<Option<Host>>>,
    source: BrowserSource,
}

struct Host {
    native: Rc<WebViewHost>,
    observer: Rc<protocol::Observer>,
}

impl BrowserView {
    pub(crate) fn open(
        context: &mut BrowserContext,
        window: &Rc<Window>,
        source: BrowserSource,
    ) -> Result<Self, String> {
        let slot = Rc::new(RefCell::new(None));
        let weak_slot = Rc::downgrade(&slot);
        let receiver = source.clone();
        let window = Rc::clone(window);
        WebViewHost::builder()
            .environment_options(context.0.clone())
            .create_for_hwnd(window.hwnd() as *mut _, move |result| {
                let Some(slot) = weak_slot.upgrade() else {
                    return;
                };
                match result.and_then(|host| install_host(host, &window, &receiver)) {
                    Ok(host) => {
                        let observer = Rc::clone(&host.observer);
                        slot.replace(Some(host));
                        observer.initialize();
                    }
                    Err(error) => {
                        receiver.unavailable("interactive verification is unavailable", error)
                    }
                }
            })
            .map_err(|error| format!("cannot create the WebView2 browser: {error}"))?;
        Ok(Self { slot, source })
    }

    /// WebView2's browser is created asynchronously and this build cannot test replacing it, so
    /// the window only navigates to the small page: the old page is left for the browser to trim.
    pub(crate) fn recycle(&mut self, _parts: RecycleParts, url: &Url) {
        self.navigate(url);
    }

    pub(crate) fn identify(&self) {
        let Some(native) = self.host() else { return };
        let source = self.source.clone();
        let receiver = source.clone();
        let weak = Rc::downgrade(&native);
        if let Err(error) = native
            .webview()
            .execute_script("navigator.userAgent", move |result| {
                match (result, weak.upgrade()) {
                    (Ok(agent), Some(native)) => identify_cookies(&native, receiver, agent),
                    (Err(error), _) => {
                        receiver.unavailable("could not read the browser identity", error)
                    }
                    (Ok(_), None) => {}
                }
            })
        {
            source.unavailable("could not read the browser identity", error);
        }
    }

    pub(crate) fn navigate(&self, url: &Url) {
        if let Some(host) = self.host() {
            if let Err(error) = host.webview().navigate(url.as_str()) {
                self.source
                    .unavailable("could not navigate to the challenge", error);
            }
        }
    }

    pub(crate) fn reveal(&self, document: DocumentId) -> bool {
        let slot = self.slot.borrow();
        slot.as_ref().is_some_and(|host| {
            host.observer.committed.get() == Some(document)
                && host.native.controller().set_visible(true).is_ok()
        })
    }

    pub(crate) fn is_current(&self, document: DocumentId) -> bool {
        self.slot
            .borrow()
            .as_ref()
            .is_some_and(|host| host.observer.committed.get() == Some(document))
    }

    pub(crate) fn evaluate(
        &self,
        script: &str,
        done: impl Fn(String) + Send + 'static,
    ) -> Result<(), String> {
        let host = self
            .host()
            .ok_or("the verification browser is unavailable")?;
        host.webview()
            .execute_script(script, move |result| done(result.unwrap_or_default()))
            .map_err(|error| error.to_string())
    }

    pub(crate) fn resize(&self, width: u32, height: u32) {
        let (Ok(width), Ok(height)) = (i32::try_from(width), i32::try_from(height)) else {
            return;
        };
        if let Some(host) = self.host() {
            if let Err(error) = host.controller().set_bounds(0, 0, width, height) {
                tracing::warn!(%error, "could not resize the verification browser");
            }
        }
    }

    fn host(&self) -> Option<Rc<WebViewHost>> {
        self.slot
            .borrow()
            .as_ref()
            .map(|host| Rc::clone(&host.native))
    }
}

fn install_host(
    native: WebViewHost,
    window: &Window,
    source: &BrowserSource,
) -> windows_webview::Result<Host> {
    let size = window.inner_size().cast::<i32>();
    native.controller().set_visible(false)?;
    native
        .controller()
        .set_bounds(0, 0, size.width, size.height)?;
    let native = Rc::new(native);
    let observer = Rc::new(protocol::Observer::new(
        source.clone(),
        Rc::downgrade(&native),
    ));
    subscribe_window(&native, &observer)?;
    protocol::subscribe(&native, &observer)?;
    Ok(Host { native, observer })
}

fn subscribe_window(
    native: &WebViewHost,
    observer: &Rc<protocol::Observer>,
) -> windows_webview::Result<()> {
    let state = Rc::clone(observer);
    let source = observer.source.clone();
    let downloads = source.clone();
    native.retain_all([
        native.webview().on_navigation_starting(move |args| {
            state.started(DocumentId(args.navigation_id()));
            if !matches!(
                Url::parse(&args.uri()).as_ref().map(Url::scheme),
                Ok("http" | "https")
            ) {
                if let Err(error) = args.set_cancel(true) {
                    state
                        .source
                        .unavailable("could not cancel a non-web navigation", error);
                }
            }
        })?,
        native.webview().on_new_window_requested(move |args| {
            if let Err(error) = args.set_handled(true) {
                source.unavailable("could not refuse a verification popup", error);
            }
        })?,
        native.webview().on_download_starting(move |args| {
            if let Err(error) = args.set_cancel(true).and_then(|()| args.set_handled(true)) {
                downloads.unavailable("could not refuse a verification download", error);
            }
        })?,
    ]);
    Ok(())
}

fn identify_cookies(native: &WebViewHost, source: BrowserSource, agent: String) {
    let receiver = source.clone();
    if let Err(error) = read_cookies(native, &source.target, move |result| match result {
        Ok(cookies) => receiver.identity(agent, cookies),
        Err(error) => receiver.unavailable("could not read the browser identity", error),
    }) {
        source.unavailable("could not read the browser identity", error);
    }
}

fn read_cookies(
    native: &WebViewHost,
    target: &Url,
    done: impl FnOnce(windows_webview::Result<Cookies>) + 'static,
) -> windows_webview::Result<()> {
    let scope = target.clone();
    native
        .webview()
        .cookie_manager()?
        .get_cookies(target.as_str(), move |result| {
            done(result.map(|cookies| {
                cookies
                    .into_iter()
                    .filter_map(|cookie| checked(webview2_parts(cookie), &scope))
                    .collect()
            }));
        })
}

/// WebView2's cookie as the fields the bridge checks (a leading dot marks a domain cookie, as in WebKit).
fn webview2_parts(cookie: windows_webview::Cookie) -> CookieParts {
    CookieParts {
        name: cookie.name,
        value: CookieValue::new(cookie.value),
        domain: cookie.domain,
        path: cookie.path,
        secure: cookie.is_secure,
        http_only: cookie.is_http_only,
        expires: expiry_from_fractional_seconds(cookie.expires),
    }
}
