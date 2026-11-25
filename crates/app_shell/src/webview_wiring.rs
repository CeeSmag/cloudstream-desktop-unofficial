//! Microsoft Edge WebView2 for the Cloudflare window: the registry probe and Microsoft's setup on
//! Windows, nothing elsewhere. `CS_WEBVIEW2_PROBE=missing|unsupported|installed` pretends a status
//! for the launch check, so the dialog can be tried on a PC that already has the runtime. The check
//! after an install always reads the real registry: pretending there made every test install end
//! in "still not installed".

use std::path::Path;
use std::sync::Arc;

use ui_slint::WebViewParts;
use webview_runtime::{
    platform_installer, platform_probe, pretended_facts, FixedProbe, RuntimeProbe,
};

/// The environment variable that pretends a WebView2 status.
pub const PRETEND_VARIABLE: &str = "CS_WEBVIEW2_PROBE";

/// The probe for `pretend` (the variable's value), or this PC's real one.
fn choose_probe(pretend: Option<&str>) -> Arc<dyn RuntimeProbe> {
    match pretend.and_then(pretended_facts) {
        Some(facts) => {
            tracing::info!(?facts, "pretending a WebView2 status ({PRETEND_VARIABLE})");
            Arc::new(FixedProbe(facts))
        }
        None => platform_probe(),
    }
}

/// WebView2's probe and installer for this PC; the setup is downloaded under `data_dir`.
pub fn build_webview(data_dir: &Path) -> WebViewParts {
    let pretend = std::env::var(PRETEND_VARIABLE).ok();
    let probe = choose_probe(pretend.as_deref());
    let installer = platform_installer(platform_probe(), data_dir.to_path_buf());
    WebViewParts { probe, installer }
}

#[cfg(test)]
mod tests {
    use super::*;
    use webview_runtime::{judge, RuntimeStatus};

    #[test]
    fn a_pretended_status_replaces_the_real_probe() {
        let probe = choose_probe(Some("unsupported"));
        assert_eq!(judge(&probe.facts()), RuntimeStatus::Unsupported);
    }

    #[test]
    fn an_unknown_or_absent_value_uses_this_pc() {
        let real = judge(&platform_probe().facts());
        assert_eq!(judge(&choose_probe(Some("maybe")).facts()), real);
        assert_eq!(judge(&choose_probe(None).facts()), real);
    }

    #[test]
    fn the_installer_exists_only_where_there_is_something_to_install() {
        let parts = build_webview(&std::env::temp_dir());
        assert_eq!(parts.installer.is_some(), cfg!(windows));
    }
}
