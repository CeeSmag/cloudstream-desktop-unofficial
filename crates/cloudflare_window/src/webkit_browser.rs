//! Hosts WebKitGTK and its native navigation observer.

#[path = "webkit_cookies.rs"]
mod cookies;
#[path = "webkit_navigation.rs"]
mod navigation;

use tao::platform::unix::WindowExtUnix;
use webkit2gtk::{SettingsExt, WebContextExt, WebViewExt};
use wry::{NewWindowResponse, WebViewBuilder, WebViewBuilderExtUnix, WebViewExtUnix};

use super::*;
use crate::verification::DocumentId;
use cookies::read_cookies;
use navigation::Observer;

/// A page with nothing in it.
const TINY_PAGE: &str = "<!doctype html><title>.</title>";

pub(crate) struct BrowserContext(wry::WebContext);

impl BrowserContext {
    pub(crate) fn new(profile: &Path) -> Self {
        Self(wry::WebContext::new(Some(profile.to_owned())))
    }
}

pub(crate) struct BrowserView {
    view: wry::WebView,
    observer: Rc<Observer>,
}

impl BrowserView {
    pub(crate) fn open(
        context: &mut BrowserContext,
        window: &Rc<Window>,
        source: BrowserSource,
    ) -> Result<Self, String> {
        let container = window
            .default_vbox()
            .ok_or("the window has no GTK container")?;
        let view = WebViewBuilder::new_with_web_context(&mut context.0)
            .with_focused(false)
            .with_visible(false)
            .with_new_window_req_handler(|_, _| NewWindowResponse::Deny)
            .with_download_started_handler(|_, _| false)
            .build_gtk(container)
            .map_err(|error| format!("cannot open the WebKit browser: {error}"))?;
        let gtk_view = view.webview();
        let cookies = gtk_view
            .context()
            .and_then(|context| context.cookie_manager())
            .ok_or("WebKit did not provide a cookie store")?;
        let observer = Observer::install(&gtk_view, source, cookies);
        observer.source.send(BrowserEvent::Ready);
        Ok(Self { view, observer })
    }

    pub(crate) fn identify(&self) {
        let source = self.observer.source.clone();
        let agent =
            WebViewExt::settings(&self.view.webview()).and_then(|settings| settings.user_agent());
        let Some(agent) = agent else {
            source.unavailable(
                "could not read the browser identity",
                "WebKit returned no user agent",
            );
            return;
        };
        read_cookies(
            &self.observer.cookies,
            &self.observer.source.target,
            move |cookies| match cookies {
                Ok(cookies) => source.identity(agent.to_string(), cookies),
                Err(error) => source.unavailable("could not read the browser identity", error),
            },
        );
    }

    pub(crate) fn navigate(&self, url: &Url) {
        if let Err(error) = self.view.load_url(url.as_str()) {
            self.observer
                .source
                .unavailable("could not navigate to the challenge", error);
        }
    }

    /// Replaces the window's browser with a new one that holds a tiny page of our own. Dropping
    /// the old one destroys its widget, which ends every process the page had (a challenge page
    /// keeps others besides the main one: ending only the main process, or navigating away, left
    /// a heavy one behind). The tiny page is given the site's origin as its base address, so it is
    /// a document of that origin and can still run the sidecar's fetches; it does not depend on
    /// what the site serves (a site's own 404 page can be a heavy app). Cookies live in the
    /// network process and are kept.
    pub(crate) fn recycle(&mut self, parts: RecycleParts<'_>, url: &Url) {
        let fresh = match Self::open(parts.context, parts.window, parts.source.clone()) {
            Ok(fresh) => fresh,
            Err(error) => {
                tracing::warn!(%error, "could not replace a verified window's browser; keeping it");
                return;
            }
        };
        let base = url.join("/").map(String::from).unwrap_or_default();
        fresh.view.webview().load_html(TINY_PAGE, Some(&base));
        *self = fresh;
    }

    pub(crate) fn reveal(&self, document: DocumentId) -> bool {
        self.is_current(document) && self.view.set_visible(true).is_ok()
    }

    pub(crate) fn is_current(&self, document: DocumentId) -> bool {
        self.observer.committed.get() == Some(document)
    }

    pub(crate) fn evaluate(
        &self,
        script: &str,
        done: impl Fn(String) + Send + 'static,
    ) -> Result<(), String> {
        self.view
            .evaluate_script_with_callback(script, done)
            .map_err(|error| error.to_string())
    }

    pub(crate) fn resize(&self, _width: u32, _height: u32) {}
}
