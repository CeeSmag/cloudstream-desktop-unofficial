//! Plain words for the WebView2 failures a viewer can fix themselves.

/// `HRESULT_FROM_WIN32(ERROR_FILE_NOT_FOUND)`: WebView2 answers this when no runtime is installed.
const RUNTIME_NOT_FOUND: i32 = 0x8007_0002_u32 as i32;

/// What the viewer is told when the Microsoft Edge WebView2 Runtime is missing.
pub const MISSING_RUNTIME: &str = "the Microsoft Edge WebView2 Runtime is not installed \
(Windows 10 or newer is needed); install it from Settings > General or from \
https://go.microsoft.com/fwlink/p/?LinkId=2124703";

/// The sentence for a WebView2 error code the viewer can act on, if it is one.
pub fn missing_runtime_reason(code: i32) -> Option<&'static str> {
    (code == RUNTIME_NOT_FOUND).then_some(MISSING_RUNTIME)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_runtime_is_named() {
        assert_eq!(
            missing_runtime_reason(-2_147_024_894),
            Some(MISSING_RUNTIME)
        );
    }

    #[test]
    fn other_errors_are_left_alone() {
        // E_ACCESSDENIED, E_ABORT, S_OK and ERROR_PATH_NOT_FOUND.
        for code in [-2_147_024_891, -2_147_467_260, 0, -2_147_024_893] {
            assert_eq!(missing_runtime_reason(code), None, "{code:#x}");
        }
    }

    #[test]
    fn the_sentence_points_at_the_fix() {
        assert!(MISSING_RUNTIME.contains("WebView2 Runtime"));
        assert!(MISSING_RUNTIME.contains("Settings > General"));
    }
}
