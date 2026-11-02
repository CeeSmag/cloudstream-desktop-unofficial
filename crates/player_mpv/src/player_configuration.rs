use crate::mpv_api::MpvHandle;
use player_api::PlayerError;

/// How the player should be configured.
#[derive(Debug, Clone, Default)]
pub struct PlayerOptions {
    /// Run without video/audio output (tests and CI).
    pub headless: bool,
    /// Deliver pictures to the app (see `VideoPlayer::latest_frame`) instead of opening mpv's window.
    pub embedded: bool,
    /// Window ID (HWND on Windows, X11 Window ID on Linux) to attach mpv to directly.
    pub wid: Option<i64>,
}

/// Options every instance gets.
const COMMON: &[(&str, &str)] = &[
    ("idle", "yes"),
    ("keep-open", "no"),
    ("osc", "no"),
    ("input-default-bindings", "no"),
    ("hwdec", "auto-copy-safe"),
    ("ytdl", "no"),
];

/// Applies the options; `embedded` says the render API will be used (no mpv window).
pub(crate) fn configure(
    handle: &MpvHandle,
    options: &PlayerOptions,
    embedded: bool,
) -> Result<(), PlayerError> {
    for (name, value) in COMMON {
        handle.set_option(name, value)?;
    }
    if std::env::var_os("CS_MPV_LOG").is_some() {
        // Developer aid: mpv's own log to the console for diagnosing stream failures.
        handle.set_option("terminal", "yes")?;
        handle.set_option("msg-level", "all=v")?;
    }
    if options.headless {
        for (name, value) in [("vo", "null"), ("ao", "null"), ("force-window", "no")] {
            handle.set_option(name, value)?;
        }
    } else if let Some(wid) = options.wid {
        let wid_str = wid.to_string();
        handle.set_option("wid", &wid_str)?;
        handle.set_option("vo", "gpu")?;
        handle.set_option("hwdec", "auto-safe")?;
    } else if embedded {
        // `vo=libmpv` renders into memory through the render API (no mpv window); `sw-fast`
        // trades a little quality for speed on the CPU renderer.
        handle.set_option("vo", "libmpv")?;
        if let Err(error) = handle.set_option("sw-fast", "yes") {
            tracing::debug!(%error, "this libmpv has no sw-fast option");
        }
    } else {
        // A real mpv window; without `force-window` it opens only when a file starts playing.
        handle.set_option("vo", "gpu")?;
        handle.set_option("osc", "yes")?;
        handle.set_option("input-default-bindings", "yes")?;
        handle.set_option("input-vo-keyboard", "yes")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn player_options_defaults_wid_to_none() {
        let options = PlayerOptions::default();
        assert!(options.wid.is_none());
        assert!(!options.headless);
        assert!(!options.embedded);
    }

    #[test]
    fn player_options_preserves_wid_value() {
        let options = PlayerOptions {
            wid: Some(12345678),
            ..PlayerOptions::default()
        };
        assert_eq!(options.wid, Some(12345678));
    }
}
