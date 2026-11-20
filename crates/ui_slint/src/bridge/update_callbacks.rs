//! Settings > Updates and the logs folder. Updates go through `app_updater`, which installs only
//! a release whose signed checksum list and SHA-256 match (the old Slint updater checked nothing).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use app_updater::{DownloadProgress, UpdateCheck};

use crate::background_work::{spawn_work, UiSender};
use crate::ui_events::{UiEvent, UpdateViewState};
use crate::CloudStream;

use super::bridge_context::BridgeContext;

/// Prevents overlapping update checks or downloads from running at the same time.
#[derive(Clone, Default)]
struct UpdateGate {
    active: Arc<AtomicBool>,
}

impl UpdateGate {
    fn try_enter(&self) -> Option<UpdateRunGuard> {
        self.active
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .ok()
            .map(|_| UpdateRunGuard {
                active: Arc::clone(&self.active),
            })
    }
}

struct UpdateRunGuard {
    active: Arc<AtomicBool>,
}

impl Drop for UpdateRunGuard {
    fn drop(&mut self) {
        self.active.store(false, Ordering::Release);
    }
}

pub(crate) fn wire_updates(ctx: &BridgeContext, ui: &CloudStream) {
    let gate = UpdateGate::default();
    // Like the old nav bar's indicator: one check per launch, a badge when a newer release exists.
    check_updates(ctx, &gate);

    let (c, g) = (ctx.clone(), gate.clone());
    ui.on_check_updates(move || check_updates(&c, &g));

    let c = ctx.clone();
    ui.on_apply_update(move || apply_update(&c, &gate));

    let c = ctx.clone();
    ui.on_open_logs(move || {
        if let Some(dir) = c.extras.borrow().data_dir.as_ref() {
            if let Err(error) = open::that(dir.join("logs")) {
                tracing::warn!(%error, "could not open the logs folder");
            }
        }
    });
}

fn read_update_state(ui: &CloudStream) -> UpdateViewState {
    UpdateViewState {
        label: ui.get_update_label().to_string(),
        badge: ui.get_update_badge().to_string(),
        version: ui.get_update_version().to_string(),
        notes: ui.get_update_notes().to_string(),
        can_apply: ui.get_update_can_apply(),
        busy: ui.get_update_busy(),
        downloading: ui.get_update_downloading(),
        progress_percent: 0,
    }
}

fn check_updates(ctx: &BridgeContext, gate: &UpdateGate) {
    let Some(guard) = gate.try_enter() else {
        return;
    };
    let mut prior = ctx
        .window()
        .map(|ui| read_update_state(&ui))
        .unwrap_or_default();
    prior.label = "Checking for updates…".to_owned();
    prior.busy = true;
    prior.downloading = false;
    prior.progress_percent = 0;

    let sender = ctx.sender();
    sender.send(UiEvent::UpdateState(prior.clone()));
    spawn_work("updates", move || {
        let _guard = guard;
        match app_updater::check_latest() {
            Ok(check) => {
                sender.send(UiEvent::UpdateState(build_update_view_state(&check)));
            }
            Err(error) => {
                tracing::warn!(%error, "update check failed");
                prior.label = format!("Update check failed: {error}");
                prior.busy = false;
                sender.send(UiEvent::UpdateState(prior));
            }
        }
    });
}

/// Builds the Settings > App updates and top-bar state from a finished GitHub check.
pub(crate) fn build_update_view_state(check: &UpdateCheck) -> UpdateViewState {
    let can_apply = check.has_update && check.asset_name.is_some();
    let notes = if check.has_update {
        check
            .release_notes
            .as_deref()
            .unwrap_or("")
            .trim()
            .to_owned()
    } else {
        String::new()
    };
    UpdateViewState {
        label: update_label(check),
        badge: update_badge(check),
        version: format_update_version(check),
        notes,
        can_apply,
        busy: false,
        downloading: false,
        progress_percent: 0,
    }
}

/// Formats the new version pill shown in Settings when a newer release exists.
pub(crate) fn format_update_version(check: &UpdateCheck) -> String {
    if !check.has_update {
        return String::new();
    }
    match check.asset_size {
        Some(bytes) if bytes > 0 => {
            format!("v{} · {}", check.latest_version, format_asset_size(bytes))
        }
        _ => format!("v{}", check.latest_version),
    }
}

fn format_asset_size(bytes: u64) -> String {
    const MIB: u64 = 1024 * 1024;
    if bytes >= MIB {
        format!("{:.1} MB", bytes as f64 / MIB as f64)
    } else {
        format!("{} KB", (bytes / 1024).max(1))
    }
}

/// The nav bar's badge: the newer version when one can be installed here, else nothing.
pub(crate) fn update_badge(check: &UpdateCheck) -> String {
    if check.has_update && check.asset_name.is_some() {
        format!("Update v{}", check.latest_version)
    } else {
        String::new()
    }
}

/// The line under "Updates" for a finished check.
pub(crate) fn update_label(check: &UpdateCheck) -> String {
    match (check.has_update, check.asset_name.is_some()) {
        (true, true) => format!(
            "Version {} is ready to install (you have {}).",
            check.latest_version, check.current_version
        ),
        (true, false) => format!(
            "{} is out, but not for this system yet.",
            check.latest_version
        ),
        (false, _) => format!("Up to date ({}).", check.current_version),
    }
}

fn apply_update(ctx: &BridgeContext, gate: &UpdateGate) {
    let Some(dir) = ctx.extras.borrow().data_dir.clone() else {
        return;
    };
    let Some(guard) = gate.try_enter() else {
        return;
    };
    let prior = ctx
        .window()
        .map(|ui| read_update_state(&ui))
        .unwrap_or_default();
    let starting = UpdateViewState {
        label: "Downloading the update…".to_owned(),
        busy: true,
        downloading: true,
        progress_percent: 0,
        ..prior.clone()
    };

    let sender = ctx.sender();
    sender.send(UiEvent::UpdateState(starting));
    let weak = ctx.ui.clone();
    spawn_work("apply-update", move || {
        let _guard = guard;
        run_apply_update(&dir, prior, &sender, &weak);
    });
}

fn run_apply_update(
    dir: &std::path::Path,
    mut prior: UpdateViewState,
    sender: &UiSender,
    weak: &slint::Weak<CloudStream>,
) {
    let mut last_percent = None;
    let mut on_progress =
        |progress: DownloadProgress| report_progress(sender, progress, &mut last_percent);
    match app_updater::download_and_install(dir, &mut on_progress) {
        Ok(()) => {
            sender.send(UiEvent::UpdateDownloadProgress {
                label: "Update checked and ready; restarting to install it…".to_owned(),
                percent: 100,
            });
            if weak
                .upgrade_in_event_loop(|_| {
                    if let Err(error) = slint::quit_event_loop() {
                        tracing::warn!(%error, "could not close for the update");
                    }
                })
                .is_err()
            {
                tracing::debug!("the window was already closed");
            }
        }
        Err(error) => {
            tracing::warn!(%error, "the update was not installed");
            prior.label = format!("Update not installed: {error}");
            prior.busy = false;
            prior.downloading = false;
            prior.progress_percent = 0;
            sender.send(UiEvent::UpdateState(prior));
        }
    }
}

fn report_progress(sender: &UiSender, progress: DownloadProgress, last: &mut Option<u8>) {
    if progress.total_bytes == 0 {
        return;
    }
    let percent = ((progress.downloaded_bytes * 100) / progress.total_bytes).min(100) as u8;
    if *last != Some(percent) {
        *last = Some(percent);
        sender.send(UiEvent::UpdateDownloadProgress {
            label: format!("Downloading the update… {percent}%"),
            percent,
        });
    }
}

#[cfg(test)]
#[path = "update_callbacks_tests.rs"]
mod tests;
