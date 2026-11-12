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
    // Prefer English audio when a stream offers several languages; see `preferred_audio`.
    ("alang", "eng,en"),
    // Read only a little ahead of the playing position. By default mpv downloads as fast as it can
    // (up to 150 MiB), which on a stream split into many small pieces is hundreds of requests; a
    // server that limits requests then refuses them ("HTTP 429") and the player skips the pieces.
    ("cache-secs", "60"),
    ("demuxer-readahead-secs", "30"),
    ("demuxer-max-bytes", "64MiB"),
    ("demuxer-max-back-bytes", "16MiB"),
];

/// Options for drawing into a window the app owns (`wid`): GPU output with hardware decoding, and
/// `force-window` so mpv's window, which paints solid black, stays when nothing plays. The app's
/// window is transparent and its page is see-through while a video plays, so mpv's window is the
/// only opaque layer behind the page: without `force-window` it disappears at the end of a video,
/// while a stream loads and when a source is switched, and the desktop shows through.
const WINDOW_OPTIONS: &[(&str, &str)] = &[
    ("vo", "gpu"),
    ("hwdec", "auto-safe"),
    ("force-window", "yes"),
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
        for (name, value) in WINDOW_OPTIONS {
            handle.set_option(name, value)?;
        }
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

    #[test]
    fn a_window_the_app_owns_keeps_mpvs_black_window_when_nothing_plays() {
        let value = |name: &str| {
            WINDOW_OPTIONS
                .iter()
                .find(|(option, _)| *option == name)
                .map(|(_, value)| *value)
        };
        assert_eq!(value("force-window"), Some("yes"));
        assert_eq!(value("vo"), Some("gpu"));
    }

    #[test]
    fn mpv_reads_only_a_little_ahead_so_a_rate_limited_server_is_not_flooded() {
        let value = |name: &str| {
            COMMON
                .iter()
                .find(|(option, _)| *option == name)
                .map(|(_, value)| *value)
        };
        assert_eq!(value("cache-secs"), Some("60"));
        assert_eq!(value("demuxer-readahead-secs"), Some("30"));
        assert_eq!(value("demuxer-max-bytes"), Some("64MiB"));
        assert_eq!(value("demuxer-max-back-bytes"), Some("16MiB"));
    }
}
