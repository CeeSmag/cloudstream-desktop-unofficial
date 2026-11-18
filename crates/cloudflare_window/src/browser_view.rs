//! Building the browser view inside a window, and reading its cookies.

use tao::event_loop::EventLoopWindowTarget;
use tao::window::{Window, WindowBuilder};
use wry::{WebContext, WebView, WebViewBuilder};

use crate::helper_events::HelperEvent;

/// A window with a browser view on `host`'s front page (never the request's own address, which can
/// be a video or an API call that a browser would download). Downloads are refused.
pub(crate) fn open_site_view(
    target: &EventLoopWindowTarget<HelperEvent>,
    context: &mut WebContext,
    host: &str,
) -> Result<(Window, WebView), String> {
    let window = WindowBuilder::new()
        .with_title(format!("Verify {host} — CloudStream"))
        .with_inner_size(tao::dpi::LogicalSize::new(960.0, 720.0))
        .build(target)
        .map_err(|error| format!("cannot open a window: {error}"))?;
    let builder = WebViewBuilder::new_with_web_context(context)
        .with_url(format!("https://{host}/"))
        .with_download_started_handler(|url, _path| {
            let url = cloudflare_window::short_address(&url);
            tracing::info!(%url, "refused a download in the verification window");
            false
        });
    let webview = build_webview(builder, &window)?;
    Ok((window, webview))
}

/// The window's cookies for `url` (names and values stay in memory, never in a log).
pub(crate) fn read_cookies(webview: &WebView, url: &str) -> Vec<(String, String)> {
    match webview.cookies_for_url(url) {
        Ok(cookies) => cookies
            .iter()
            .filter(|cookie| !cookie.value().is_empty())
            .map(|cookie| (cookie.name().to_owned(), cookie.value().to_owned()))
            .collect(),
        Err(error) => {
            tracing::warn!(%error, "cannot read the window's cookies");
            Vec::new()
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn build_webview(builder: WebViewBuilder<'_>, window: &Window) -> Result<WebView, String> {
    builder
        .build(window)
        .map_err(|error| format!("cannot open the browser view: {error}"))
}

#[cfg(target_os = "linux")]
fn build_webview(builder: WebViewBuilder<'_>, window: &Window) -> Result<WebView, String> {
    use tao::platform::unix::WindowExtUnix;
    use wry::WebViewBuilderExtUnix;
    let vbox = window
        .default_vbox()
        .ok_or_else(|| "the window has no GTK container".to_owned())?;
    builder
        .build_gtk(vbox)
        .map_err(|error| format!("cannot open the browser view: {error}"))
}
