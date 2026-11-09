//! Opening the place where the Cloudflare check runs: a kept one is reused, else a new one is built.
//! The first way that works is used: inside the app's window (a panel), then a window of its own
//! browser profile, then a window in the app's profile.

use std::path::Path;

use tauri::{AppHandle, Manager, Url, Webview, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use crate::cloudflare_window::short;

/// How the check is opened.
#[derive(Debug, Clone, Copy)]
enum Way {
    /// A window with its own browser profile (WebView2 refuses another window with other options in one).
    OwnProfile,
    /// A window in the app's profile, with the same browser options as the app's main window.
    AppProfile,
}

/// Where a check runs.
pub enum Surface {
    /// A window of its own.
    Window(Box<WebviewWindow>),
}

impl Surface {
    /// The browser view, to look at its address and cookies and to run scripts in.
    pub fn view(&self) -> &Webview {
        match self {
            Self::Window(window) => (**window).as_ref(),
        }
    }

    /// Takes it off screen but keeps the page alive, for requests made in it later.
    pub fn keep_hidden(&self) {
        let _ = match self {
            Self::Window(window) => window.hide(),
        };
    }

    /// Closes it.
    pub fn dispose(&self) {
        let _ = match self {
            Self::Window(window) => window.destroy(),
        };
    }
}

/// Closes the kept check named `label`, if there is one.
pub fn close_kept(app: &AppHandle, label: &str) {
    if let Some(surface) = find(app, label) {
        surface.dispose();
    }
}

/// The kept check named `label`.
pub fn find(app: &AppHandle, label: &str) -> Option<Surface> {
    app.get_webview_window(label)
        .map(|window| Surface::Window(Box::new(window)))
}

/// The site's check from an earlier verification (kept hidden), brought back on the challenged page.
/// A window cannot be closed and re-created under the same label at once, so it is reused.
pub fn reuse(app: &AppHandle, label: &str, url: &Url) -> Option<Surface> {
    let surface = find(app, label)?;
    match surface.view().navigate(url.clone()) {
        Ok(()) => {
            let Surface::Window(window) = &surface;
            let _ = window.show();
            let _ = window.set_focus();
            tracing::info!(%label, page = %short(url.as_str()), "reusing the kept check of this site");
            Some(surface)
        }
        Err(error) => {
            tracing::warn!(%label, error = ?error, "cannot reuse the kept check; it is closed and a new one is tried");
            surface.dispose();
            None
        }
    }
}

/// Tries each way of opening the check until one works; the reason lists every failure.
pub fn open(
    app: &AppHandle,
    target: (&Url, &str),
    browser_profile: &Path,
) -> Result<Surface, String> {
    let mut failures = Vec::new();
    for way in [Way::OwnProfile, Way::AppProfile] {
        match build(app, target, browser_profile, way) {
            Ok(surface) => {
                tracing::info!(?way, page = %short(target.0.as_str()), "the Cloudflare check is open");
                return Ok(surface);
            }
            Err(error) => {
                tracing::warn!(?way, error = ?error, "cannot open the Cloudflare check this way");
                failures.push(format!("{way:?}: {error}"));
            }
        }
    }
    Err(format!("cannot open the check ({})", failures.join("; ")))
}

fn build(
    app: &AppHandle,
    target: (&Url, &str),
    browser_profile: &Path,
    way: Way,
) -> tauri::Result<Surface> {
    let (url, label) = target;
    let host = url.host_str().unwrap_or("the site");
    let builder = WebviewWindowBuilder::new(app, label, WebviewUrl::External(url.clone()))
        .title(format!("Verify {host} - CloudStream"))
        .inner_size(560.0, 780.0)
        .center()
        .focused(true);
    let builder = match way {
        Way::AppProfile => with_main_window_options(app, builder),
        _ => builder.data_directory(browser_profile.to_path_buf()),
    };
    builder
        .build()
        .map(|window| Surface::Window(Box::new(window)))
}

/// The browser options of the app's own window, so a window in its profile does not clash with it.
#[cfg(windows)]
fn with_main_window_options<'a>(
    app: &AppHandle,
    builder: WebviewWindowBuilder<'a, tauri::Wry, AppHandle>,
) -> WebviewWindowBuilder<'a, tauri::Wry, AppHandle> {
    let options = app
        .config()
        .app
        .windows
        .first()
        .and_then(|window| window.additional_browser_args.clone());
    match options {
        Some(options) => builder.additional_browser_args(&options),
        None => builder,
    }
}

#[cfg(not(windows))]
fn with_main_window_options<'a>(
    _app: &AppHandle,
    builder: WebviewWindowBuilder<'a, tauri::Wry, AppHandle>,
) -> WebviewWindowBuilder<'a, tauri::Wry, AppHandle> {
    builder
}
