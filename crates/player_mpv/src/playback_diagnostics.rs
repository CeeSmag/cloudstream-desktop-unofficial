//! What mpv says about the stream it is playing: decoder, codec, frame rates, picture size and
//! video output. Written to the log once per stream (see `diagnostics_timer`) so a "choppy video"
//! report can be told from a slow decoder, a software fallback or a wrong video output.

use std::fmt;

/// How mpv decodes the video.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Decoding {
    /// By the graphics hardware, through the named API (`d3d11va`, `d3d11va-copy`, ...).
    Hardware(String),
    /// On the processor (mpv answers `no`).
    Software,
    /// mpv has not said yet (no video, or the decoder is not set up).
    Unknown,
}

impl fmt::Display for Decoding {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Hardware(api) => write!(out, "hardware ({api})"),
            Self::Software => out.write_str("software"),
            Self::Unknown => out.write_str("unknown"),
        }
    }
}

/// One reading of the properties above.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PlaybackDiagnostics {
    pub(crate) decoding: Decoding,
    pub(crate) codec: Option<String>,
    pub(crate) container_fps: Option<f64>,
    pub(crate) estimated_fps: Option<f64>,
    pub(crate) size: Option<(u32, u32)>,
    pub(crate) video_output: Option<String>,
}

fn text(get: &impl Fn(&str) -> Option<String>, name: &str) -> Option<String> {
    get(name)
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn number<T: std::str::FromStr>(get: &impl Fn(&str) -> Option<String>, name: &str) -> Option<T> {
    text(get, name)?.parse().ok()
}

fn frame_rate(get: &impl Fn(&str) -> Option<String>, name: &str) -> Option<f64> {
    number::<f64>(get, name).filter(|fps| fps.is_finite() && *fps > 0.0)
}

/// Reads the diagnostics through `get`, which answers an mpv property name with its text.
pub(crate) fn read_diagnostics(get: impl Fn(&str) -> Option<String>) -> PlaybackDiagnostics {
    let decoding = match text(&get, "hwdec-current").as_deref() {
        None => Decoding::Unknown,
        Some("no") => Decoding::Software,
        Some(api) => Decoding::Hardware(api.to_owned()),
    };
    let width = number::<u32>(&get, "video-params/w").filter(|w| *w > 0);
    let height = number::<u32>(&get, "video-params/h").filter(|h| *h > 0);
    PlaybackDiagnostics {
        decoding,
        codec: text(&get, "video-codec"),
        container_fps: frame_rate(&get, "container-fps"),
        estimated_fps: frame_rate(&get, "estimated-vf-fps"),
        size: width.zip(height),
        video_output: text(&get, "current-vo"),
    }
}

fn fps_text(fps: Option<f64>) -> String {
    fps.map_or_else(|| "unknown".to_owned(), |fps| format!("{fps:.2}"))
}

fn size_text(size: Option<(u32, u32)>) -> String {
    size.map_or_else(
        || "unknown".to_owned(),
        |(width, height)| format!("{width}x{height}"),
    )
}

impl PlaybackDiagnostics {
    /// True when the stream has a picture, which is when the figures mean anything.
    pub(crate) fn has_video(&self) -> bool {
        self.codec.is_some() || self.size.is_some()
    }

    /// True for a video that mpv decodes on the processor.
    pub(crate) fn is_decoded_in_software(&self) -> bool {
        self.has_video() && self.decoding == Decoding::Software
    }

    /// Writes the figures at `info` (so they reach the main log), and a warning when the video is
    /// decoded in software, which is the usual reason a high-resolution stream stutters.
    pub(crate) fn log(&self, generation: u64) {
        let codec = self.codec.as_deref().unwrap_or("none");
        tracing::info!(
            generation,
            decoding = %self.decoding,
            codec,
            container_fps = %fps_text(self.container_fps),
            estimated_fps = %fps_text(self.estimated_fps),
            size = %size_text(self.size),
            video_output = self.video_output.as_deref().unwrap_or("none"),
            "playback diagnostics"
        );
        if self.is_decoded_in_software() {
            tracing::warn!(
                generation,
                codec,
                size = %size_text(self.size),
                "the video is decoded in software; the graphics hardware is not helping"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn properties(entries: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = entries
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect();
        move |name| map.get(name).cloned()
    }

    const HARDWARE_VIDEO: &[(&str, &str)] = &[
        ("hwdec-current", "d3d11va"),
        ("video-codec", "H.264 / AVC (h264)"),
        ("container-fps", "23.976024"),
        ("estimated-vf-fps", "23.97"),
        ("video-params/w", "1920"),
        ("video-params/h", "1080"),
        ("current-vo", "gpu"),
    ];

    #[test]
    fn read_diagnostics_hardware_video_returns_every_figure() {
        let found = read_diagnostics(properties(HARDWARE_VIDEO));
        assert_eq!(found.decoding, Decoding::Hardware("d3d11va".to_owned()));
        assert_eq!(found.codec.as_deref(), Some("H.264 / AVC (h264)"));
        assert_eq!(found.size, Some((1920, 1080)));
        assert_eq!(found.video_output.as_deref(), Some("gpu"));
        assert!((found.container_fps.unwrap_or(0.0) - 23.976).abs() < 0.001);
        assert!(!found.is_decoded_in_software());
    }

    #[test]
    fn read_diagnostics_hwdec_no_returns_software() {
        let found = read_diagnostics(properties(&[
            ("hwdec-current", "no"),
            ("video-codec", "AV1 (av1)"),
            ("video-params/w", "3840"),
            ("video-params/h", "2160"),
        ]));
        assert_eq!(found.decoding, Decoding::Software);
        assert!(found.is_decoded_in_software());
    }

    #[test]
    fn read_diagnostics_nothing_known_returns_unknown_and_no_video() {
        let found = read_diagnostics(properties(&[]));
        assert_eq!(found.decoding, Decoding::Unknown);
        assert_eq!(found.size, None);
        assert!(!found.has_video());
        assert!(!found.is_decoded_in_software());
    }

    #[test]
    fn an_audio_only_stream_is_never_called_software_decoded() {
        let found = read_diagnostics(properties(&[("hwdec-current", "no")]));
        assert_eq!(found.decoding, Decoding::Software);
        assert!(!found.has_video());
        assert!(!found.is_decoded_in_software());
    }

    #[test]
    fn read_diagnostics_ignores_empty_zero_and_garbled_values() {
        let found = read_diagnostics(properties(&[
            ("hwdec-current", "  "),
            ("container-fps", "0.000000"),
            ("estimated-vf-fps", "fast"),
            ("video-params/w", "0"),
            ("video-params/h", "1080"),
            ("current-vo", ""),
        ]));
        assert_eq!(found.decoding, Decoding::Unknown);
        assert_eq!(found.container_fps, None);
        assert_eq!(found.estimated_fps, None);
        assert_eq!(found.size, None, "a zero width is no size");
        assert_eq!(found.video_output, None);
    }

    #[test]
    fn the_figures_read_as_plain_text() {
        assert_eq!(fps_text(Some(23.976_024)), "23.98");
        assert_eq!(fps_text(None), "unknown");
        assert_eq!(size_text(Some((1280, 720))), "1280x720");
        assert_eq!(size_text(None), "unknown");
        assert_eq!(
            Decoding::Hardware("d3d11va-copy".to_owned()).to_string(),
            "hardware (d3d11va-copy)"
        );
    }
}
