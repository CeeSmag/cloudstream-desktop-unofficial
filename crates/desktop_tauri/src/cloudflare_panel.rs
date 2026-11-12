//! The Cloudflare check inside the app's own window: a second browser view on top of the app page, placed
//! over a frame that the page draws. The page measures the frame and tells the backend where it is
//! (`place_cloudflare_panel`); the backend moves the view there.
//!
//! Every step is logged to `cloudflare-logs.txt`: building the view, each new place, the viewer's cancel,
//! a placement that never comes, and every failure. Commands are `async` on purpose: building or moving
//! views from a synchronous command can deadlock WebView2 and freeze the whole app (see `deadlock_tests`).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{
    AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, State, Webview, WebviewBuilder,
    WebviewUrl,
};

/// The page event that opens or closes the frame.
pub const PANEL_EVENT: &str = "cloudflare-panel";
/// Where a view waits while no frame shows it: off the window's corner, so the page keeps running.
/// The app's own window.
const MAIN: &str = "main";
const PARKED_AT: f64 = -400.0;
const PARKED_SIZE: f64 = 2.0;
/// No frame is bigger than this (CSS pixels); anything else is a bad measurement.
const LIMIT: f64 = 20_000.0;
/// A move or resize slower than this is reported: it is how a stuck window shows up.
const SLOW: Duration = Duration::from_secs(2);

/// What the page is told about the frame.
#[derive(Debug, Clone, Serialize)]
pub struct PanelPayload {
    pub open: bool,
    pub host: String,
}

/// Where the frame is, in CSS pixels of the app page.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect {
    /// A usable place, or the reason the numbers cannot be one.
    pub fn checked(numbers: [f64; 4]) -> Result<Self, String> {
        let [x, y, width, height] = numbers;
        if numbers.iter().any(|n| !n.is_finite()) {
            return Err(format!("the frame's place is not a number: {numbers:?}"));
        }
        if width < 1.0 || height < 1.0 {
            return Err(format!("the frame is empty ({width} x {height})"));
        }
        if width > LIMIT || height > LIMIT || x.abs() > LIMIT || y.abs() > LIMIT {
            return Err(format!("the frame's place is out of range: {numbers:?}"));
        }
        Ok(Self {
            x,
            y,
            width,
            height,
        })
    }
}

/// What the viewer and the page tell the waiting check: where the frame is, and that it was cancelled.
#[derive(Default)]
pub struct PanelControl {
    cancelled: AtomicBool,
    placed: Mutex<Option<Rect>>,
}

impl PanelControl {
    /// A new check starts: no cancel, no place yet.
    pub fn begin(&self) {
        self.cancelled.store(false, Ordering::SeqCst);
        self.set_placed(None);
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    pub fn set_placed(&self, rect: Option<Rect>) {
        if let Ok(mut placed) = self.placed.lock() {
            *placed = rect;
        }
    }

    pub fn placed(&self) -> Option<Rect> {
        self.placed.lock().ok().and_then(|placed| *placed)
    }
}

/// Tells the page to show or hide the frame; a failure is logged (the check then runs unseen).
pub fn announce(app: &AppHandle, host: &str, open: bool) {
    let payload = PanelPayload {
        open,
        host: host.to_owned(),
    };
    match app.emit(PANEL_EVENT, payload) {
        Ok(()) => {
            tracing::info!(%host, open, "told the app page to {} the check frame", if open { "show" } else { "hide" })
        }
        Err(error) => {
            tracing::error!(%host, open, %error, "cannot tell the app page about the check frame; the check may be invisible")
        }
    }
}

/// How the check view's browser is set up.
pub struct Options<'a> {
    /// Its own browser profile folder, or the app's profile when `None`.
    pub profile: Option<&'a std::path::Path>,
    /// Browser arguments (they must match the app's when the profile is shared).
    pub browser_args: Option<String>,
}

/// Builds the check view in the app's main window, parked until the page places it.
pub fn build(
    app: &AppHandle,
    target: (&tauri::Url, &str),
    options: Options<'_>,
) -> tauri::Result<Webview> {
    let (url, label) = target;
    let window = app.get_window(MAIN).ok_or(tauri::Error::WindowNotFound)?;
    let mut builder = WebviewBuilder::new(label, WebviewUrl::External(url.clone()));
    if let Some(profile) = options.profile {
        builder = builder.data_directory(profile.to_path_buf());
    }
    #[cfg(windows)]
    if let Some(args) = options.browser_args.as_deref() {
        builder = builder.additional_browser_args(args);
    }
    window.add_child(
        builder,
        LogicalPosition::new(PARKED_AT, PARKED_AT),
        LogicalSize::new(PARKED_SIZE, PARKED_SIZE),
    )
}

/// Moves the view off the visible window (it keeps running there).
pub fn park(view: &Webview) {
    let moved = view
        .set_position(LogicalPosition::new(PARKED_AT, PARKED_AT))
        .and_then(|()| view.set_size(LogicalSize::new(PARKED_SIZE, PARKED_SIZE)));
    match moved {
        Ok(()) => tracing::info!(label = view.label(), "the check view is parked off screen"),
        Err(error) => {
            tracing::warn!(label = view.label(), %error, "cannot park the check view; it may stay on screen")
        }
    }
}

/// Moves the view over the frame.
fn place(view: &Webview, rect: Rect) -> tauri::Result<()> {
    view.set_position(LogicalPosition::new(rect.x, rect.y))?;
    view.set_size(LogicalSize::new(rect.width, rect.height))
}

/// The page says where the frame is; the check view moves there.
#[tauri::command]
pub async fn place_cloudflare_panel(
    host: String,
    rect: [f64; 4],
    app: AppHandle,
    control: State<'_, Arc<PanelControl>>,
) -> Result<(), String> {
    let rect = Rect::checked(rect).inspect_err(|reason| {
        tracing::warn!(%host, %reason, "the app page sent a place that cannot be used");
    })?;
    let label = crate::cloudflare_window::label_for(&host);
    let Some(view) = app.get_webview(&label) else {
        let reason = format!("there is no check view for {host} to place");
        tracing::warn!(%host, %label, "{reason}");
        return Err(reason);
    };
    let started = Instant::now();
    place(&view, rect).map_err(|error| {
        tracing::error!(%host, ?rect, %error, "cannot move the check view over the frame");
        format!("cannot show the check: {error}")
    })?;
    if started.elapsed() > SLOW {
        tracing::warn!(%host, took_ms = started.elapsed().as_millis() as u64, "moving the check view was slow");
    }
    if control.placed() != Some(rect) {
        tracing::info!(%host, ?rect, "the check view is now over the app page's frame");
    }
    control.set_placed(Some(rect));
    Ok(())
}

/// The viewer pressed Cancel on the frame.
#[tauri::command]
pub async fn cancel_cloudflare_panel(
    host: String,
    app: AppHandle,
    control: State<'_, Arc<PanelControl>>,
) -> Result<(), String> {
    tracing::info!(%host, "the viewer cancelled the Cloudflare check");
    control.cancel();
    // Park the view now: when no check is waiting (the site showed no challenge) nothing else would,
    // and the view would stay over the page with no way to close it.
    if let Some(view) = app.get_webview(&crate::cloudflare_window::label_for(&host)) {
        park(&view);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_normal_frame_is_accepted() {
        let rect = Rect::checked([10.0, 20.0, 600.0, 400.0]).expect("usable");
        assert_eq!((rect.x, rect.width), (10.0, 600.0));
    }

    #[test]
    fn bad_measurements_are_refused_with_a_reason() {
        for bad in [
            [f64::NAN, 0.0, 10.0, 10.0],
            [0.0, 0.0, f64::INFINITY, 10.0],
            [0.0, 0.0, 0.0, 10.0],
            [0.0, 0.0, 10.0, 0.5],
            [0.0, 0.0, 30_000.0, 10.0],
            [-50_000.0, 0.0, 10.0, 10.0],
        ] {
            assert!(Rect::checked(bad).is_err(), "{bad:?} must be refused");
        }
    }

    #[test]
    fn a_new_check_forgets_the_old_cancel_and_place() {
        let control = PanelControl::default();
        control.cancel();
        control.set_placed(Rect::checked([1.0, 1.0, 5.0, 5.0]).ok());
        control.begin();
        assert!(!control.is_cancelled());
        assert_eq!(control.placed(), None);
    }

    #[test]
    fn cancel_and_place_from_many_threads_never_lose_the_cancel() {
        let control = Arc::new(PanelControl::default());
        let workers: Vec<_> = (0..8)
            .map(|n| {
                let control = Arc::clone(&control);
                std::thread::spawn(move || {
                    for i in 0..500 {
                        control
                            .set_placed(Rect::checked([f64::from(n), f64::from(i), 9.0, 9.0]).ok());
                        let _ = control.placed();
                        if n == 3 && i == 250 {
                            control.cancel();
                        }
                    }
                })
            })
            .collect();
        for worker in workers {
            worker.join().expect("worker");
        }
        assert!(
            control.is_cancelled(),
            "a cancel must survive concurrent placing"
        );
        assert!(control.placed().is_some());
    }
}
