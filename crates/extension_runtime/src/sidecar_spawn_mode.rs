//! Chooses whether a sidecar JVM gets the OS filesystem jail (AppContainer / Landlock).

use crate::SidecarLaunchConfig;

/// Production JVM launches set `cloudstream.data.dir` via `with_data_dir`; tests use `fake_sidecar`.
pub(crate) fn config_requests_data_dir(config: &SidecarLaunchConfig) -> bool {
    config
        .environment
        .iter()
        .any(|(name, value)| name == "JAVA_OPTS" && value.contains("-Dcloudstream.data.dir="))
}

/// True when this worker should use AppContainer (Windows) or Landlock (Linux).
///
/// Trusted plugins keep `data.dir` and memory caps but turn `os_fs_sandbox` off so they skip
/// the FS jail; Job Object / rlimit still apply on the plain-with-limits path.
pub(crate) fn uses_os_fs_sandbox(config: &SidecarLaunchConfig) -> bool {
    config.os_fs_sandbox && config_requests_data_dir(config)
}

/// True when Linux should still apply rlimit + process-group without Landlock.
#[cfg(any(test, target_os = "linux"))]
pub(crate) fn uses_linux_process_limits(config: &SidecarLaunchConfig) -> bool {
    config_requests_data_dir(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn production_untrusted_uses_os_fs_sandbox() {
        let config = SidecarLaunchConfig::for_install(Path::new("/opt/sidecar"), None)
            .with_data_dir(Path::new("/data/sidecar"));
        assert!(config.os_fs_sandbox);
        assert!(uses_os_fs_sandbox(&config));
        assert!(uses_linux_process_limits(&config));
    }

    #[test]
    fn trusted_keeps_data_dir_but_skips_os_fs_sandbox() {
        let config = SidecarLaunchConfig::for_install(Path::new("/opt/sidecar"), None)
            .with_data_dir(Path::new("/data/sidecar"))
            .without_os_fs_sandbox();
        assert!(config_requests_data_dir(&config));
        assert!(!uses_os_fs_sandbox(&config));
        assert!(uses_linux_process_limits(&config));
    }

    #[test]
    fn fake_sidecar_without_data_dir_stays_plain() {
        let plain = SidecarLaunchConfig::for_install(Path::new("/opt/sidecar"), None);
        assert!(!config_requests_data_dir(&plain));
        assert!(!uses_os_fs_sandbox(&plain));
        assert!(!uses_linux_process_limits(&plain));
    }
}
