//! T1 tests for the WebView2 dialog rules.

use super::*;

fn installed() -> RuntimeStatus {
    RuntimeStatus::Installed {
        version: "131.0.2903.70".into(),
    }
}

fn offer(status: RuntimeStatus) -> DialogView {
    dialog_view(&DialogState::Offer(status))
}

fn phase(phase: InstallPhase) -> DialogView {
    dialog_view(&DialogState::Install(phase))
}

#[test]
fn a_missing_runtime_offers_the_install() {
    let view = offer(RuntimeStatus::Missing);
    assert_eq!(view.title, "WebView2 is needed for Cloudflare checks");
    assert!(view.message.contains("not installed on this PC"));
    assert_eq!(view.primary, Some("Install WebView2"));
    assert_eq!(view.close_label, "Not now");
    assert!(view.can_close && view.show_download_link && !view.show_progress);
}

#[test]
fn old_windows_is_told_why_with_nothing_to_install() {
    let view = offer(RuntimeStatus::Unsupported);
    assert_eq!(view.title, "Cloudflare checks need Windows 10 or newer");
    assert!(view.message.contains("Everything else"));
    assert_eq!(view.primary, None);
    assert_eq!(view.close_label, "Close");
    assert!(view.can_close && !view.show_download_link);
}

#[test]
fn an_installed_runtime_has_nothing_to_offer() {
    for status in [installed(), RuntimeStatus::NotNeeded] {
        let view = offer(status);
        assert_eq!(view.title, "WebView2 is ready");
        assert_eq!(view.primary, None);
        assert!(!view.show_download_link);
    }
}

#[test]
fn the_download_shows_its_percent_and_can_be_cancelled() {
    let view = phase(InstallPhase::Downloading {
        done: 21,
        total: 50,
    });
    assert_eq!(view.title, "Installing WebView2");
    assert_eq!(view.message, "Downloading Microsoft's setup… 42 %");
    assert!(view.show_progress && !view.indeterminate);
    assert!((view.progress - 0.42).abs() < 1e-6);
    assert_eq!(view.close_label, "Cancel");
    assert!(view.can_close && view.primary.is_none());
}

#[test]
fn a_download_of_unknown_size_is_indeterminate() {
    let view = phase(InstallPhase::Downloading { done: 21, total: 0 });
    assert_eq!(view.message, "Downloading Microsoft's setup…");
    assert!(view.show_progress && view.indeterminate);
    assert_eq!(view.progress, 0.0);
}

#[test]
fn the_signature_check_can_still_be_cancelled() {
    let view = phase(InstallPhase::Verifying);
    assert!(view.message.contains("signed by Microsoft"));
    assert!(view.indeterminate && view.can_close);
    assert_eq!(view.close_label, "Cancel");
}

#[test]
fn the_running_setup_cannot_be_closed() {
    let view = phase(InstallPhase::Installing);
    assert!(view.message.contains("keep CloudStream open"));
    assert!(view.indeterminate && view.show_progress);
    assert!(!view.can_close);
    assert_eq!(view.close_label, "Close");
}

#[test]
fn a_finished_install_says_no_restart_is_needed() {
    let view = phase(InstallPhase::Done(installed()));
    assert_eq!(view.title, "WebView2 is ready");
    assert!(view.message.contains("No restart"));
    assert_eq!(view.close_label, "Done");
    assert!(view.can_close && !view.show_progress && view.primary.is_none());
}

#[test]
fn a_failure_explains_itself_and_offers_both_ways_out() {
    let view = phase(InstallPhase::Failed(SetupError::InstallerFailed(5)));
    assert_eq!(view.title, "WebView2 was not installed");
    assert_eq!(
        view.message,
        "The WebView2 setup stopped with code 5. You can try again, or install it yourself from \
         Microsoft's website."
    );
    assert_eq!(view.primary, Some("Try again"));
    assert!(view.show_download_link && view.can_close && !view.show_progress);
}

#[test]
fn a_cancelled_install_offers_to_start_again() {
    let view = phase(InstallPhase::Failed(SetupError::Cancelled));
    assert_eq!(view.title, "Install stopped");
    assert_eq!(view.message, "Nothing was installed.");
    assert_eq!(view.primary, Some("Install WebView2"));
    assert!(!view.show_download_link);
}

#[test]
fn sentences_start_upper_case() {
    assert_eq!(sentence_case("could not"), "Could not");
    assert_eq!(sentence_case("élan"), "Élan");
    assert_eq!(sentence_case(""), "");
}

#[test]
fn the_launch_prompt_opens_only_when_it_helps() {
    let missing = RuntimeStatus::Missing;
    assert!(should_prompt_at_home(Some(&missing), false));
    assert!(should_prompt_at_home(
        Some(&RuntimeStatus::Unsupported),
        false
    ));
    assert!(
        !should_prompt_at_home(Some(&missing), true),
        "already shown this run"
    );
    assert!(!should_prompt_at_home(None, false), "not probed yet");
    assert!(!should_prompt_at_home(Some(&installed()), false));
    assert!(!should_prompt_at_home(
        Some(&RuntimeStatus::NotNeeded),
        false
    ));
}

#[test]
fn the_settings_row_follows_the_status() {
    assert_eq!(settings_row(None, false), None);
    assert_eq!(settings_row(Some(&RuntimeStatus::NotNeeded), false), None);
    let missing = settings_row(Some(&RuntimeStatus::Missing), false).unwrap();
    assert!(missing.can_install && missing.text.starts_with("Not installed"));
    let old = settings_row(Some(&RuntimeStatus::Unsupported), false).unwrap();
    assert!(!old.can_install && old.text.contains("Windows 10"));
    assert_eq!(
        settings_row(Some(&installed()), false),
        Some(SettingsRow {
            text: "Installed (version 131.0.2903.70).".into(),
            can_install: false
        })
    );
}

#[test]
fn the_settings_row_shows_a_running_install_whatever_the_status() {
    for status in [
        None,
        Some(RuntimeStatus::Missing),
        Some(RuntimeStatus::NotNeeded),
    ] {
        assert_eq!(
            settings_row(status.as_ref(), true),
            Some(SettingsRow {
                text: "Installing WebView2…".into(),
                can_install: false
            })
        );
    }
}
