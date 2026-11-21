//! Building the browser view inside a window, and reading its cookies.

use tao::event_loop::EventLoopWindowTarget;
use tao::window::{Window, WindowBuilder};
use wry::{WebContext, WebView, WebViewBuilder};

use crate::helper_events::HelperEvent;

/// A window with a browser view on `page_url` (the challenged page when safe). Starts hidden so a
/// quick auto-pass never bothers the viewer. Downloads are refused.
pub(crate) fn open_site_view(
    target: &EventLoopWindowTarget<HelperEvent>,
    context: &mut WebContext,
    host: &str,
    page_url: &str,
) -> Result<(Window, WebView), String> {
    let window = WindowBuilder::new()
        .with_title(format!("Verify {host} — CloudStream"))
        .with_inner_size(tao::dpi::LogicalSize::new(960.0, 720.0))
        .with_focused(false)
        .with_visible(false)
        .build(target)
        .map_err(|error| format!("cannot open a window: {error}"))?;
    // Start blank so TLS-ignore can be applied on the shared WebKit context before the first
    // navigation; otherwise a self-signed redirect paints WebKit's error page immediately.
    let builder = WebViewBuilder::new_with_web_context(context)
        .with_focused(false)
        .with_url("about:blank")
        .with_download_started_handler(|url, _path| {
            let url = cloudflare_window::short_address(&url);
            tracing::info!(%url, "refused a download in the verification window");
            false
        });
    let builder = configure_verify_builder(builder);
    let webview = build_webview(builder, &window)?;
    ignore_verify_tls_errors(&webview);
    webview
        .load_url(page_url)
        .map_err(|error| format!("cannot load the verification page: {error}"))?;
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

/// Verify windows must still open when a site redirects to a self-signed host (parked domain /
/// CDN hop). On Linux that means telling WebKit to ignore TLS errors for this shared context.
#[cfg(target_os = "linux")]
fn ignore_verify_tls_errors(webview: &WebView) {
    use webkit2gtk::{WebContextExt as _, WebViewExt as _, WebsiteDataManagerExt};
    use wry::WebViewExtUnix;
    let gtk_view = webview.webview();
    let Some(manager) = gtk_view
        .context()
        .and_then(|context| context.website_data_manager())
    else {
        return;
    };
    manager.set_tls_errors_policy(webkit2gtk::TLSErrorsPolicy::Ignore);
}

#[cfg(not(target_os = "linux"))]
fn ignore_verify_tls_errors(_webview: &WebView) {}

/// Browser flags for Windows WebView2 so verification does not stall on self-signed or
/// mismatched-authority redirect hosts (`net::ERR_CERT_AUTHORITY_INVALID`).
#[cfg(any(target_os = "windows", test))]
const WINDOWS_VERIFY_BROWSER_ARGS: &str = "--ignore-certificate-errors";

#[cfg(target_os = "windows")]
fn configure_verify_builder(builder: WebViewBuilder<'_>) -> WebViewBuilder<'_> {
    use wry::WebViewBuilderExtWindows;
    builder.with_additional_browser_args(WINDOWS_VERIFY_BROWSER_ARGS)
}

#[cfg(not(target_os = "windows"))]
fn configure_verify_builder(builder: WebViewBuilder<'_>) -> WebViewBuilder<'_> {
    builder
}

#[cfg(test)]
mod tests {
    use super::WINDOWS_VERIFY_BROWSER_ARGS;

    #[test]
    fn windows_verify_browser_args_ignore_certificate_errors() {
        assert!(WINDOWS_VERIFY_BROWSER_ARGS.contains("--ignore-certificate-errors"));
    }
}
