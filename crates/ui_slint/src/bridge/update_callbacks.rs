//! Settings > Updates and the logs folder. Updates go through `app_updater`, which installs only
//! a release whose signed checksum list and SHA-256 match (the old Slint updater checked nothing).

use app_updater::{DownloadProgress, UpdateCheck};

use crate::background_work::{spawn_work, UiSender};
use crate::ui_events::UiEvent;
use crate::CloudStream;

use super::bridge_context::BridgeContext;

pub(crate) fn wire_updates(ctx: &BridgeContext, ui: &CloudStream) {
    // Like the old nav bar's indicator: one check per launch, a badge when a newer release exists.
    check_updates(ctx);
    let c = ctx.clone();
    ui.on_check_updates(move || check_updates(&c));
    let c = ctx.clone();
    ui.on_apply_update(move || apply_update(&c));
    let c = ctx.clone();
    ui.on_open_logs(move || {
        if let Some(dir) = c.extras.borrow().data_dir.as_ref() {
            if let Err(error) = open::that(dir.join("logs")) {
                tracing::warn!(%error, "could not open the logs folder");
            }
        }
    });
}

fn check_updates(ctx: &BridgeContext) {
    if let Some(ui) = ctx.window() {
        ui.set_update_label("Checking for updates…".into());
    }
    let sender = ctx.sender();
    spawn_work("updates", move || match app_updater::check_latest() {
        Ok(check) => {
            sender.send(UiEvent::UpdateLabel(update_label(&check)));
            sender.send(UiEvent::UpdateBadge(update_badge(&check)));
        }
        Err(error) => {
            tracing::warn!(%error, "update check failed");
            sender.send(UiEvent::UpdateLabel(format!(
                "Update check failed: {error}"
            )));
        }
    });
}

/// The nav bar's badge: the newer version when one can be installed here, else nothing.
pub(crate) fn update_badge(check: &UpdateCheck) -> String {
    if check.has_update && check.asset_name.is_some() {
        format!("Update {}", check.latest_version)
    } else {
        String::new()
    }
}

/// The line under "Updates" for a finished check.
pub(crate) fn update_label(check: &UpdateCheck) -> String {
    match (check.has_update, check.asset_name.is_some()) {
        (true, true) => format!(
            "Update available: {} (you have {}). Press Apply update.",
            check.latest_version, check.current_version
        ),
        (true, false) => format!(
            "{} is out, but not for this system yet.",
            check.latest_version
        ),
        (false, _) => format!("Up to date ({}).", check.current_version),
    }
}

fn apply_update(ctx: &BridgeContext) {
    let Some(dir) = ctx.extras.borrow().data_dir.clone() else {
        return;
    };
    if let Some(ui) = ctx.window() {
        ui.set_update_label("Downloading the update…".into());
    }
    let sender = ctx.sender();
    let weak = ctx.ui.clone();
    spawn_work("apply-update", move || {
        let mut last_percent = None;
        let mut on_progress =
            |progress: DownloadProgress| report_progress(&sender, progress, &mut last_percent);
        match app_updater::download_and_install(&dir, &mut on_progress) {
            Ok(()) => {
                sender.send(UiEvent::UpdateLabel(
                    "Update checked and ready; restarting to install it…".into(),
                ));
                let closing = weak.upgrade_in_event_loop(|_| {
                    if let Err(error) = slint::quit_event_loop() {
                        tracing::warn!(%error, "could not close for the update");
                    }
                });
                if closing.is_err() {
                    tracing::debug!("the window was already closed");
                }
            }
            Err(error) => {
                tracing::warn!(%error, "the update was not installed");
                sender.send(UiEvent::UpdateLabel(format!(
                    "Update not installed: {error}"
                )));
            }
        }
    });
}

fn report_progress(sender: &UiSender, progress: DownloadProgress, last: &mut Option<u64>) {
    if progress.total_bytes == 0 {
        return;
    }
    let percent = progress.downloaded_bytes * 100 / progress.total_bytes;
    if *last != Some(percent) {
        *last = Some(percent);
        sender.send(UiEvent::UpdateLabel(format!(
            "Downloading the update… {percent}%"
        )));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(has_update: bool, asset: bool) -> UpdateCheck {
        UpdateCheck {
            current_version: "0.4.8".into(),
            latest_version: "0.5.0".into(),
            has_update,
            asset_name: asset.then(|| "CloudStream-0.5.0-x86_64.AppImage".to_owned()),
            asset_size: None,
            release_name: None,
            release_notes: None,
        }
    }

    #[test]
    fn update_label_says_what_to_do() {
        assert!(update_label(&check(true, true)).starts_with("Update available: 0.5.0"));
        assert!(update_label(&check(true, false)).contains("not for this system"));
        assert_eq!(update_label(&check(false, true)), "Up to date (0.4.8).");
    }

    #[test]
    fn update_badge_only_for_an_installable_newer_release() {
        assert_eq!(update_badge(&check(true, true)), "Update 0.5.0");
        assert_eq!(update_badge(&check(true, false)), "");
        assert_eq!(update_badge(&check(false, true)), "");
    }
}
