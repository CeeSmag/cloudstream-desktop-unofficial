//! The Cloudflare verification helper's logic that needs no window (tested on its own).
#![forbid(unsafe_code)]

pub mod fetch_script;
pub mod verify_open_url;
pub mod verify_progress;

/// A web address without its query or fragment, for the log (signed addresses stay private).
pub fn short_address(url: &str) -> String {
    url.split(['?', '#']).next().unwrap_or(url).to_owned()
}

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

    #[test]
    fn short_address_drops_query_and_fragment() {
        assert_eq!(
            short_address("https://a.test/v.m3u8?token=x#y"),
            "https://a.test/v.m3u8"
        );
    }
}
