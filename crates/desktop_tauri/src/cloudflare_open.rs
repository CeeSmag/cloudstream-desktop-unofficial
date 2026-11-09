//! Opening the place where the Cloudflare check runs: a kept one is reused, else a new one is built.
//! The first way that works is used: inside the app's window (a panel) with its own browser profile,
//! then a panel in the app's profile, then a window of its own profile, then a window in the app's profile.

use std::path::Path;

use tauri::{AppHandle, Manager, Url, Webview, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use crate::cloudflare_panel::{self, Options};
use crate::cloudflare_window::short;

/// How the check is opened.
#[derive(Debug, Clone, Copy)]
enum Way {
    /// Inside the app's window, with its own browser profile.
    PanelOwn,
    /// Inside the app's window, sharing the app's profile and browser options.
    PanelApp,
    /// A window with its own browser profile (WebView2 refuses another window with other options in one).
    WindowOwn,
    /// A window in the app's profile, with the same browser options as the app's main window.
    WindowApp,
}

/// Where a check runs.
pub enum Surface {
    /// A view inside the app's own window.
    Panel(Box<Webview>),
    /// A window of its own.
    Window(Box<WebviewWindow>),
}

impl Surface {
    /// The browser view, to look at its address and cookies and to run scripts in.
    pub fn view(&self) -> &Webview {
        match self {
            Self::Panel(view) => view,
            Self::Window(window) => (**window).as_ref(),
        }
    }

    /// Whether the check shows inside the app's window.
    pub fn is_panel(&self) -> bool {
        matches!(self, Self::Panel(_))
    }

    /// Takes it off screen but keeps the page alive, for requests made in it later.
    pub fn keep_hidden(&self) {
        match self {
            Self::Panel(view) => cloudflare_panel::park(view),
            Self::Window(window) => log_failure("hide", window.hide()),
        }
    }

    /// A check that was not passed: a window is closed; a view stays parked (its label stays taken until
    /// the app closes it, so the next check of that site reuses it).
    pub fn release(&self) {
        match self {
            Self::Panel(view) => cloudflare_panel::park(view),
            Self::Window(window) => log_failure("destroy", window.destroy()),
        }
    }

    /// Closes it for good.
    pub fn dispose(&self) {
        match self {
            Self::Panel(view) => log_failure("close", view.close()),
            Self::Window(window) => log_failure("destroy", window.destroy()),
        }
    }
}

fn log_failure(what: &str, result: tauri::Result<()>) {
    if let Err(error) = result {
        tracing::warn!(what, %error, "cannot change the Cloudflare check");
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
    if let Some(window) = app.get_webview_window(label) {
        return Some(Surface::Window(Box::new(window)));
    }
    app.get_webview(label)
        .map(|view| Surface::Panel(Box::new(view)))
}

/// The site's check from an earlier verification (kept hidden), brought back on the challenged page.
/// A window cannot be closed and re-created under the same label at once, so it is reused.
pub fn reuse(app: &AppHandle, label: &str, url: &Url) -> Option<Surface> {
    let surface = find(app, label)?;
    match surface.view().navigate(url.clone()) {
        Ok(()) => {
            if let Surface::Window(window) = &surface {
                log_failure("show", window.show());
                log_failure("focus", window.set_focus());
            }
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
    for way in [Way::PanelOwn, Way::PanelApp, Way::WindowOwn, Way::WindowApp] {
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
    let options = match way {
        Way::PanelOwn => Options {
            profile: Some(browser_profile),
            browser_args: None,
        },
        Way::PanelApp => Options {
            profile: None,
            browser_args: main_window_options(app),
        },
        Way::WindowOwn | Way::WindowApp => return build_window(app, target, browser_profile, way),
    };
    cloudflare_panel::build(app, target, options).map(|view| Surface::Panel(Box::new(view)))
}

fn build_window(
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
        Way::WindowApp => with_main_window_options(app, builder),
        _ => builder.data_directory(browser_profile.to_path_buf()),
    };
    builder
        .build()
        .map(|window| Surface::Window(Box::new(window)))
}

/// The browser options of the app's own window (none when the config has none).
fn main_window_options(app: &AppHandle) -> Option<String> {
    app.config()
        .app
        .windows
        .first()
        .and_then(|window| window.additional_browser_args.clone())
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
