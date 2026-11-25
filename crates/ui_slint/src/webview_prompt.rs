//! What the WebView2 dialog and its Settings row say, and when the dialog opens by itself. Pure
//! rules; `bridge/webview_dialog.rs` only copies them into the `WebViewDialog` Slint global.

use webview_runtime::{InstallPhase, RuntimeStatus, SetupError};

/// What the dialog is showing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DialogState {
    /// Explaining the status and offering what can be done about it.
    Offer(RuntimeStatus),
    /// An install that is running or has just ended.
    Install(InstallPhase),
}

/// Everything the dialog shows, ready for Slint.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DialogView {
    pub(crate) title: String,
    pub(crate) message: String,
    /// The main button, `None` when there is nothing to do.
    pub(crate) primary: Option<&'static str>,
    pub(crate) close_label: &'static str,
    /// Microsoft's setup cannot be stopped half-way, so Close is off while it runs.
    pub(crate) can_close: bool,
    pub(crate) show_progress: bool,
    /// 0 to 1.
    pub(crate) progress: f32,
    pub(crate) indeterminate: bool,
    /// Offer Microsoft's page for installing by hand.
    pub(crate) show_download_link: bool,
}

const INSTALL: &str = "Install WebView2";
const MISSING_TEXT: &str = "Some sites protect their videos with a Cloudflare check. CloudStream \
opens that check in Microsoft Edge WebView2, which is not installed on this PC.\n\nInstall it now? \
Microsoft's own setup downloads and installs it; no restart is needed.";
const UNSUPPORTED_TEXT: &str = "Sites that use a Cloudflare check cannot be opened on this \
version of Windows: Microsoft Edge WebView2 needs Windows 10 or newer. Everything else in \
CloudStream works as usual.";

fn view(title: &str, message: String) -> DialogView {
    DialogView {
        title: title.to_owned(),
        message,
        primary: None,
        close_label: "Close",
        can_close: true,
        show_progress: false,
        progress: 0.0,
        indeterminate: false,
        show_download_link: false,
    }
}

/// The dialog for `state`.
pub(crate) fn dialog_view(state: &DialogState) -> DialogView {
    match state {
        DialogState::Offer(status) => offer_view(status),
        DialogState::Install(phase) => install_view(phase),
    }
}

fn offer_view(status: &RuntimeStatus) -> DialogView {
    match status {
        RuntimeStatus::Missing => DialogView {
            primary: Some(INSTALL),
            close_label: "Not now",
            show_download_link: true,
            ..view(
                "WebView2 is needed for Cloudflare checks",
                MISSING_TEXT.into(),
            )
        },
        RuntimeStatus::Unsupported => view(
            "Cloudflare checks need Windows 10 or newer",
            UNSUPPORTED_TEXT.into(),
        ),
        RuntimeStatus::Installed { .. } | RuntimeStatus::NotNeeded => view(
            "WebView2 is ready",
            "Cloudflare checks can open on this PC.".into(),
        ),
    }
}

fn running(message: String, progress: Option<u8>, can_close: bool) -> DialogView {
    DialogView {
        close_label: if can_close { "Cancel" } else { "Close" },
        can_close,
        show_progress: true,
        progress: progress.map_or(0.0, |share| f32::from(share) / 100.0),
        indeterminate: progress.is_none(),
        ..view("Installing WebView2", message)
    }
}

fn install_view(phase: &InstallPhase) -> DialogView {
    match phase {
        InstallPhase::Downloading { .. } => {
            let message = match phase.percent() {
                Some(share) => format!("Downloading Microsoft's setup… {share} %"),
                None => "Downloading Microsoft's setup…".to_owned(),
            };
            running(message, phase.percent(), true)
        }
        InstallPhase::Verifying => running(
            "Checking that the setup is signed by Microsoft…".into(),
            None,
            true,
        ),
        InstallPhase::Installing => running(
            "Microsoft's setup is installing WebView2. This can take a few minutes; please keep \
             CloudStream open."
                .into(),
            None,
            false,
        ),
        InstallPhase::Done(_) => DialogView {
            close_label: "Done",
            ..view(
                "WebView2 is ready",
                "Cloudflare checks work now. No restart is needed.".into(),
            )
        },
        InstallPhase::Failed(error) => failed_view(error),
    }
}

fn failed_view(error: &SetupError) -> DialogView {
    if *error == SetupError::Cancelled {
        return DialogView {
            primary: Some(INSTALL),
            ..view("Install stopped", "Nothing was installed.".into())
        };
    }
    DialogView {
        primary: Some("Try again"),
        show_download_link: true,
        ..view(
            "WebView2 was not installed",
            format!(
                "{}. You can try again, or install it yourself from Microsoft's website.",
                sentence_case(&error.to_string())
            ),
        )
    }
}

/// `text` with its first letter upper-case.
fn sentence_case(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

/// Whether the dialog opens by itself once Home is up: when the runtime blocks the Cloudflare
/// check and it has not been shown yet in this run. Closing it is not remembered: every launch
/// that finds the runtime missing asks again.
pub(crate) fn should_prompt_at_home(status: Option<&RuntimeStatus>, already_shown: bool) -> bool {
    !already_shown && status.is_some_and(RuntimeStatus::blocks_verification)
}

/// The Settings row: `None` hides it (nothing to show on this PC).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SettingsRow {
    pub(crate) text: String,
    pub(crate) can_install: bool,
}

/// The Settings row for `status`, while an install is (or is not) running.
pub(crate) fn settings_row(
    status: Option<&RuntimeStatus>,
    installing: bool,
) -> Option<SettingsRow> {
    let row = |text: &str, can_install: bool| SettingsRow {
        text: text.to_owned(),
        can_install,
    };
    if installing {
        return Some(row("Installing WebView2…", false));
    }
    match status? {
        RuntimeStatus::NotNeeded => None,
        RuntimeStatus::Missing => Some(row(
            "Not installed: sites with a Cloudflare check cannot be opened.",
            true,
        )),
        RuntimeStatus::Unsupported => Some(row("Needs Windows 10 or newer.", false)),
        RuntimeStatus::Installed { version } => {
            Some(row(&format!("Installed (version {version})."), false))
        }
    }
}

#[cfg(test)]
#[path = "webview_prompt_tests.rs"]
mod tests;
