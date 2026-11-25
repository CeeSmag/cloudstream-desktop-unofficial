//! The Cloudflare verification helper's logic that needs no window (tested on its own).
#![forbid(unsafe_code)]

pub mod fetch_script;
pub mod runtime_errors;

/// A script's string answer: WebView2 hands it JSON-encoded (`"text"`), WebKitGTK plain (`text`).
pub fn script_text(raw: &str) -> String {
    serde_json::from_str::<String>(raw).unwrap_or_else(|_| raw.trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_text_accepts_json_and_plain_answers() {
        assert_eq!(script_text("\"Mozilla/5.0 (X11)\""), "Mozilla/5.0 (X11)");
        assert_eq!(script_text("Mozilla/5.0 (X11)\n"), "Mozilla/5.0 (X11)");
        assert_eq!(script_text(""), "");
    }
}
