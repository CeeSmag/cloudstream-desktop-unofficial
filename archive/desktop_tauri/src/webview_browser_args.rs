//! Platform WebView Chromium flags for the main window.
//!
//! Windows keeps `--disable-gpu` (fullscreen mouse regression). Linux drops it so
//! WebKit/Chromium can composite on the GPU; anti-throttling flags stay on both.

/// Chromium flags from `tauri.conf.json`, with `--disable-gpu` only on Windows.
pub fn for_platform() -> &'static str {
    #[cfg(windows)]
    {
        WINDOWS_ARGS
    }
    #[cfg(not(windows))]
    {
        LINUX_ARGS
    }
}

/// Same line as `tauri.conf.json` `additionalBrowserArgs` (Windows default).
#[cfg(windows)]
const WINDOWS_ARGS: &str = concat!(
    "--disable-gpu ",
    "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection,CalculateNativeWinOcclusion ",
    "--disable-background-timer-throttling ",
    "--disable-renderer-backgrounding ",
    "--disable-backgrounding-occluded-windows"
);

/// Linux: no `--disable-gpu`; keep the anti-throttling and feature flags.
#[cfg(not(windows))]
const LINUX_ARGS: &str = concat!(
    "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection,CalculateNativeWinOcclusion ",
    "--disable-background-timer-throttling ",
    "--disable-renderer-backgrounding ",
    "--disable-backgrounding-occluded-windows"
);

/// Writes the platform args onto every configured window before the app builds.
pub fn apply_to_config(config: &mut tauri::Config) {
    let args = for_platform().to_owned();
    for window in &mut config.app.windows {
        window.additional_browser_args = Some(args.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_args_keep_anti_throttling() {
        let args = for_platform();
        assert!(args.contains("--disable-background-timer-throttling"));
        assert!(args.contains("--disable-renderer-backgrounding"));
        assert!(args.contains("--disable-backgrounding-occluded-windows"));
        assert!(args.contains("CalculateNativeWinOcclusion"));
    }

    #[test]
    #[cfg(windows)]
    fn windows_keeps_disable_gpu() {
        assert!(for_platform().contains("--disable-gpu"));
    }

    #[test]
    #[cfg(not(windows))]
    fn linux_drops_disable_gpu() {
        assert!(!for_platform().contains("--disable-gpu"));
    }
}
