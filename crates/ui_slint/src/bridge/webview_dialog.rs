//! The Microsoft Edge WebView2 dialog: the launch check, the prompt after Home, the install with
//! its progress, the Settings row, and the prompt when a Cloudflare check fails for want of it.
//!
//! What it says and when it opens by itself are the pure rules of `webview_prompt.rs`; this file
//! only runs them. The status lives in a [`StatusBoard`] so a slow launch probe cannot overwrite
//! what the probe after an install found, and [`InstallGate`] lets one install run at a time.

use std::sync::Arc;

use slint::ComponentHandle;
use webview_runtime::{
    judge, CancelFlag, InstallGate, InstallPhase, ProgressThrottle, RuntimeInstaller, RuntimeProbe,
    RuntimeStatus, StatusBoard, DOWNLOAD_PAGE,
};

use crate::background_work::{spawn_work, UiSender};
use crate::ui_events::UiEvent;
use crate::webview_prompt::{dialog_view, settings_row, should_prompt_at_home, DialogState};
use crate::{CloudStream, WebViewDialog};

use super::bridge_context::BridgeContext;

/// WebView2 for the Cloudflare window, built by `app_shell` for this PC.
pub struct WebViewParts {
    /// Reads whether the runtime is installed.
    pub probe: Arc<dyn RuntimeProbe>,
    /// Installs it; `None` where there is nothing to install.
    pub installer: Option<RuntimeInstaller>,
}

/// What the UI thread keeps about WebView2.
#[derive(Default)]
pub(crate) struct WebViewSession {
    board: Arc<StatusBoard>,
    gate: InstallGate,
    cancel: Option<CancelFlag>,
    dialog: Option<DialogState>,
    home_arrived: bool,
    shown_at_home: bool,
    shown_for_cloudflare: bool,
}

/// News from the WebView2 workers.
#[derive(Debug)]
pub(crate) enum WebViewEvent {
    /// A probe wrote a new status to the board.
    StatusChanged,
    /// The running install moved on (or ended).
    Phase(InstallPhase),
}

/// Connects the dialog's buttons.
pub(crate) fn wire_webview_dialog(ctx: &BridgeContext, ui: &CloudStream) {
    let dialog = ui.global::<WebViewDialog>();
    let c = ctx.clone();
    dialog.on_primary(move || with_window(&c, primary));
    let c = ctx.clone();
    dialog.on_close(move || with_window(&c, close));
    let c = ctx.clone();
    dialog.on_show(move || with_window(&c, show_from_settings));
    dialog.on_open_download_page(|| {
        if let Err(error) = open::that(DOWNLOAD_PAGE) {
            tracing::warn!(%error, "cannot open Microsoft's WebView2 page");
        }
    });
}

fn with_window(ctx: &BridgeContext, action: fn(&BridgeContext, &CloudStream)) {
    if let Some(ui) = ctx.window() {
        action(ctx, &ui);
    }
}

fn probe(ctx: &BridgeContext) -> Option<Arc<dyn RuntimeProbe>> {
    ctx.extras
        .borrow()
        .webview
        .as_ref()
        .map(|parts| Arc::clone(&parts.probe))
}

fn current_status(ctx: &BridgeContext) -> Option<RuntimeStatus> {
    ctx.session.borrow().webview.board.current()
}

/// Reads the registry in the background while the app starts (nothing is shown for it: it takes
/// far less than a frame); the result is used once Home is up.
pub(crate) fn start_webview_probe(ctx: &BridgeContext) {
    let Some(probe) = probe(ctx) else {
        return;
    };
    let board = Arc::clone(&ctx.session.borrow().webview.board);
    let sender = ctx.sender();
    spawn_work("webview-probe", move || {
        refresh_status(&board, &*probe, &sender)
    });
}

/// Probes and, when this probe is still the newest, tells the UI.
fn refresh_status(board: &StatusBoard, probe: &dyn RuntimeProbe, sender: &UiSender) {
    let ticket = board.begin_probe();
    let status = judge(&probe.facts());
    tracing::info!(?status, "Microsoft Edge WebView2 runtime");
    if board.finish_probe(ticket, status) {
        sender.send(UiEvent::WebView(WebViewEvent::StatusChanged));
    }
}

/// Home is on screen: the launch prompt may open now. It opens on every launch that finds the
/// runtime missing; closing it is not remembered.
pub(crate) fn on_home_arrived(ctx: &BridgeContext, ui: &CloudStream) {
    ctx.session.borrow_mut().webview.home_arrived = true;
    maybe_prompt_at_home(ctx, ui);
}

fn maybe_prompt_at_home(ctx: &BridgeContext, ui: &CloudStream) {
    let status = current_status(ctx);
    let (arrived, shown, open) = {
        let session = ctx.session.borrow();
        let state = &session.webview;
        (
            state.home_arrived,
            state.shown_at_home,
            state.dialog.is_some(),
        )
    };
    if !arrived || open || !should_prompt_at_home(status.as_ref(), shown) {
        return;
    }
    if let Some(status) = status {
        ctx.session.borrow_mut().webview.shown_at_home = true;
        show(ctx, ui, DialogState::Offer(status));
    }
}

/// A Cloudflare check failed: if WebView2 is why, explain it (once per run).
pub(crate) fn on_cloudflare_failed(ctx: &BridgeContext, ui: &CloudStream) {
    let blocks = current_status(ctx).filter(RuntimeStatus::blocks_verification);
    let mut session = ctx.session.borrow_mut();
    let state = &mut session.webview;
    let Some(status) = blocks.filter(|_| state.dialog.is_none() && !state.shown_for_cloudflare)
    else {
        return;
    };
    state.shown_for_cloudflare = true;
    drop(session);
    show(ctx, ui, DialogState::Offer(status));
}

fn show_from_settings(ctx: &BridgeContext, ui: &CloudStream) {
    let status = current_status(ctx).unwrap_or(RuntimeStatus::Missing);
    show(ctx, ui, DialogState::Offer(status));
}

/// Applies a worker's news.
pub(crate) fn show_webview_event(ctx: &BridgeContext, ui: &CloudStream, event: WebViewEvent) {
    match event {
        WebViewEvent::StatusChanged => {
            let status = current_status(ctx);
            let offer = matches!(
                ctx.session.borrow().webview.dialog,
                Some(DialogState::Offer(_))
            );
            if let (true, Some(status)) = (offer, status) {
                show(ctx, ui, DialogState::Offer(status));
            }
            maybe_prompt_at_home(ctx, ui);
        }
        WebViewEvent::Phase(phase) => {
            let installing = matches!(
                ctx.session.borrow().webview.dialog,
                Some(DialogState::Install(_))
            );
            if phase.is_finished() {
                ctx.session.borrow_mut().webview.cancel = None;
            }
            if installing {
                show(ctx, ui, DialogState::Install(phase));
            }
        }
    }
    render_row(ctx, ui);
}

/// Opens (or updates) the dialog with `state`.
fn show(ctx: &BridgeContext, ui: &CloudStream, state: DialogState) {
    let view = dialog_view(&state);
    ctx.session.borrow_mut().webview.dialog = Some(state);
    let dialog = ui.global::<WebViewDialog>();
    dialog.set_title(view.title.into());
    dialog.set_message(view.message.into());
    dialog.set_primary_label(view.primary.unwrap_or_default().into());
    dialog.set_close_label(view.close_label.into());
    dialog.set_can_close(view.can_close);
    dialog.set_show_progress(view.show_progress);
    dialog.set_progress(view.progress);
    dialog.set_indeterminate(view.indeterminate);
    dialog.set_show_download_link(view.show_download_link);
    dialog.set_open(true);
}

fn render_row(ctx: &BridgeContext, ui: &CloudStream) {
    let row = {
        let session = ctx.session.borrow();
        settings_row(
            session.webview.board.current().as_ref(),
            session.webview.gate.is_busy(),
        )
    };
    let dialog = ui.global::<WebViewDialog>();
    dialog.set_row_visible(row.is_some());
    if let Some(row) = row {
        dialog.set_row_text(row.text.into());
        dialog.set_row_can_install(row.can_install);
    }
}

/// Close / Not now / Cancel / Done. During the download Cancel stops it; while Microsoft's setup
/// runs nothing closes the dialog.
pub(crate) fn close(ctx: &BridgeContext, ui: &CloudStream) {
    let mut session = ctx.session.borrow_mut();
    let state = &mut session.webview;
    match &state.dialog {
        Some(DialogState::Install(phase)) if !phase.is_finished() && !phase.can_cancel() => return,
        Some(DialogState::Install(phase)) if phase.can_cancel() => {
            if let Some(cancel) = &state.cancel {
                cancel.cancel();
            }
        }
        _ => {}
    }
    state.dialog = None;
    drop(session);
    ui.global::<WebViewDialog>().set_open(false);
}

fn primary(ctx: &BridgeContext, ui: &CloudStream) {
    let can_start = match &ctx.session.borrow().webview.dialog {
        Some(DialogState::Offer(status)) => status.can_install(),
        Some(DialogState::Install(InstallPhase::Failed(_))) => true,
        _ => false,
    };
    if can_start {
        start_install(ctx, ui);
    }
}

/// Starts the install unless one is already running.
fn start_install(ctx: &BridgeContext, ui: &CloudStream) {
    let parts = {
        let extras = ctx.extras.borrow();
        extras
            .webview
            .as_ref()
            .and_then(|parts| Some((parts.installer.clone()?, Arc::clone(&parts.probe))))
    };
    let Some((installer, probe)) = parts else {
        return;
    };
    let job = {
        let mut session = ctx.session.borrow_mut();
        let state = &mut session.webview;
        let Some(run) = state.gate.try_enter() else {
            return;
        };
        let cancel = CancelFlag::new();
        state.cancel = Some(cancel.clone());
        InstallJob {
            installer,
            probe,
            board: Arc::clone(&state.board),
            cancel,
            run,
        }
    };
    show(
        ctx,
        ui,
        DialogState::Install(InstallPhase::Downloading { done: 0, total: 0 }),
    );
    render_row(ctx, ui);
    let sender = ctx.sender();
    spawn_work("webview-install", move || job.run(&sender));
}

struct InstallJob {
    installer: RuntimeInstaller,
    probe: Arc<dyn RuntimeProbe>,
    board: Arc<StatusBoard>,
    cancel: CancelFlag,
    run: webview_runtime::InstallRun,
}

impl InstallJob {
    fn run(self, sender: &UiSender) {
        let mut throttle = ProgressThrottle::new();
        let result = self.installer.install(&self.cancel, &mut |phase| {
            if throttle.admit(&phase) {
                sender.send(UiEvent::WebView(WebViewEvent::Phase(phase)));
            }
        });
        if let Err(error) = &result {
            tracing::warn!(%error, "Microsoft Edge WebView2 was not installed");
        }
        refresh_status(&self.board, &*self.probe, sender);
        // The gate opens before the UI hears the end, so "Try again" pressed on the failure
        // message always finds it open.
        drop(self.run);
        sender.send(UiEvent::WebView(WebViewEvent::Phase(
            InstallPhase::finished(result),
        )));
    }
}

/// Modal keys: everything goes to the dialog; "back" closes it.
pub(crate) fn dialog_takes_key(ctx: &BridgeContext, ui: &CloudStream, key: &str) -> bool {
    if !ui.global::<WebViewDialog>().get_open() {
        return false;
    }
    if key == "back" {
        close(ctx, ui);
    }
    true
}

#[cfg(test)]
#[path = "webview_dialog_tests.rs"]
mod tests;
