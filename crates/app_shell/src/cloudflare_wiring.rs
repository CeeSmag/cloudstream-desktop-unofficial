//! Starts the Cloudflare verification watcher and finds its helper program.

use std::path::{Path, PathBuf};

use app_services::SettingsService;
use cloudflare_bridge::{spawn_watcher, AllowedHosts, HelperLaunch, WatchConfig, WatchHandle};

/// Overrides where the verification helper is (for development builds).
const HELPER_VARIABLE: &str = "CS_VERIFY_HELPER";

/// The helper's file name next to the app's program.
fn helper_file_name() -> &'static str {
    if cfg!(windows) {
        "cloudstream-verify.exe"
    } else {
        "cloudstream-verify"
    }
}

/// Starts watching `<data>/sidecar/cloudflare`, with the sites the viewer allowed before.
pub fn start_cloudflare_watch(data_dir: &Path, settings: &SettingsService) -> WatchHandle {
    let exchange = data_dir.join("sidecar").join("cloudflare");
    cloudflare_bridge::clear_folder(&exchange);
    let saved = settings.cloudflare_allowed_hosts().unwrap_or_else(|error| {
        tracing::warn!(%error, "could not read the allowed Cloudflare sites; asking again");
        Vec::new()
    });
    let helper = find_helper().map(|program| HelperLaunch {
        program,
        args: helper_args(&exchange, &data_dir.join("cloudflare_browser")),
    });
    if helper.is_none() {
        tracing::warn!(
            "no Cloudflare verification helper next to the program; such sites cannot be verified"
        );
    }
    let watch = spawn_watcher(WatchConfig {
        dir: exchange,
        allowed: AllowedHosts::new(saved),
        helper,
    });
    let idle = settings
        .helper_idle()
        .map(memory_budget::HelperIdle::as_duration)
        .unwrap_or_else(|error| {
            tracing::warn!(%error, "verification helper idle time not read; keeping it open");
            None
        });
    watch.set_helper_idle(idle);
    watch
}

fn helper_args(exchange: &Path, profile: &Path) -> Vec<String> {
    vec![
        "--exchange".to_owned(),
        exchange.to_string_lossy().into_owned(),
        "--profile".to_owned(),
        profile.to_string_lossy().into_owned(),
    ]
}

/// `CS_VERIFY_HELPER`, else `cloudstream-verify` next to the running program.
fn find_helper() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os(HELPER_VARIABLE).map(PathBuf::from) {
        return path.is_file().then_some(path);
    }
    let program = std::env::current_exe().ok()?;
    let helper = program.parent()?.join(helper_file_name());
    helper.is_file().then_some(helper)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helper_args_name_both_folders() {
        let args = helper_args(
            Path::new("/d/sidecar/cloudflare"),
            Path::new("/d/cloudflare_browser"),
        );
        assert_eq!(
            args,
            [
                "--exchange",
                "/d/sidecar/cloudflare",
                "--profile",
                "/d/cloudflare_browser"
            ]
        );
    }
}
