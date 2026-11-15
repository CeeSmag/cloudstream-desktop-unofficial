//! Recognises mpv log lines that say hardware video decoding or the graphics device broke.
//!
//! mpv has no event for this: the only trace is a log line. The phrases below are the wording
//! libmpv 2 uses (checked against the strings inside `libmpv-2.dll`); a different wording simply
//! goes unrecognised and the line is still written to the log as usual.

/// What went wrong with hardware decoding, as far as mpv's log tells.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HwdecProblem {
    /// The graphics device was lost, removed or reset (a driver crash, a GPU switch, a sleep).
    DeviceLost,
    /// Hardware decoding could not be set up for this stream, or a decoded picture could not be used.
    DecoderFailed,
}

impl HwdecProblem {
    /// A short sentence for the log.
    pub(crate) fn describe(self) -> &'static str {
        match self {
            Self::DeviceLost => "the graphics device was lost or reset",
            Self::DecoderFailed => "hardware decoding failed for this stream",
        }
    }
}

/// Lower-case phrases that mean the graphics device is gone.
const DEVICE_LOST_PHRASES: &[&str] = &[
    "device lost",
    "device removed",
    "device reset",
    "device hung",
];

/// Lower-case phrases that mean hardware decoding did not work for the stream.
const DECODER_FAILED_PHRASES: &[&str] = &[
    "hardware decoding of this stream is unsupported",
    "initializing texture for hardware decoding failed",
    "failed mapping hwdec frame",
    "hwaccel initialisation returned error",
    "failed to create d3d11va device",
    "failed to create hw frame context",
];

/// Tells whether one mpv log line (its text, without the prefix) reports a hardware-decoder problem.
pub(crate) fn classify_log(text: &str) -> Option<HwdecProblem> {
    let text = text.to_lowercase();
    let mentions = |phrases: &[&str]| phrases.iter().any(|phrase| text.contains(phrase));
    if mentions(DEVICE_LOST_PHRASES) {
        Some(HwdecProblem::DeviceLost)
    } else if mentions(DECODER_FAILED_PHRASES) {
        Some(HwdecProblem::DecoderFailed)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_direct3d_device_lost_returns_device_lost() {
        assert_eq!(
            classify_log("Direct3D device lost! Resetting."),
            Some(HwdecProblem::DeviceLost)
        );
    }

    #[test]
    fn classify_dxgi_style_removal_returns_device_lost() {
        assert_eq!(
            classify_log("Device removed: DXGI_ERROR_DEVICE_REMOVED"),
            Some(HwdecProblem::DeviceLost)
        );
        assert_eq!(
            classify_log("Direct3D device reset"),
            Some(HwdecProblem::DeviceLost)
        );
    }

    #[test]
    fn classify_unsupported_stream_returns_decoder_failed() {
        assert_eq!(
            classify_log("Hardware decoding of this stream is unsupported?"),
            Some(HwdecProblem::DecoderFailed)
        );
    }

    #[test]
    fn classify_texture_and_mapping_failures_return_decoder_failed() {
        assert_eq!(
            classify_log("Initializing texture for hardware decoding failed."),
            Some(HwdecProblem::DecoderFailed)
        );
        assert_eq!(
            classify_log("Failed mapping hwdec frame? Open a bug!"),
            Some(HwdecProblem::DecoderFailed)
        );
    }

    #[test]
    fn classify_ignores_case() {
        assert_eq!(classify_log("DEVICE LOST"), Some(HwdecProblem::DeviceLost));
    }

    #[test]
    fn classify_normal_hardware_decoding_lines_returns_none() {
        for line in [
            "Using hardware decoding (d3d11va).",
            "Trying hardware decoding via d3d11va.",
            "Using software decoding.",
            "No hardware decoding requested.",
            "Looking at hwdec d3d11va-copy...",
            "Failed to create 11_1+ device, trying 11_0",
            "",
        ] {
            assert_eq!(classify_log(line), None, "{line:?}");
        }
    }

    #[test]
    fn device_lost_wins_when_a_line_names_both() {
        assert_eq!(
            classify_log("Device lost! Hardware decoding of this stream is unsupported?"),
            Some(HwdecProblem::DeviceLost)
        );
    }
}
