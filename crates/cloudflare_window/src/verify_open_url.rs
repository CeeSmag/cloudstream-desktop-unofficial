//! Which address the verification window loads.
//!
//! The sidecar names the URL that returned the challenge. Opening that page shows Cloudflare's
//! interstitial when one is in play. Media/API paths fall back to the site's front page so the
//! browser does not try to download a stream.

use crate::verify_progress::{host_of, is_on_site};

/// Extensions that are video, audio or raw segments — never open these as the verify page.
const MEDIA_SUFFIXES: &[&str] = &[
    ".m3u8", ".mpd", ".mp4", ".mkv", ".webm", ".ts", ".m4s", ".m4a", ".mp3", ".aac", ".flac",
    ".jpg", ".jpeg", ".png", ".gif", ".webp", ".vtt", ".srt",
];

/// The URL to load for `host` given the sidecar's challenged address.
pub fn page_to_open(host: &str, challenge_url: &str) -> String {
    let front = format!("https://{host}/");
    if !is_http_url(challenge_url) {
        return front;
    }
    let challenged_host = host_of(challenge_url);
    if challenged_host.is_empty() || !is_on_site(challenged_host, host) {
        return front;
    }
    if looks_like_media(challenge_url) {
        return front;
    }
    challenge_url.to_owned()
}

fn is_http_url(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    lower.starts_with("https://") || lower.starts_with("http://")
}

fn looks_like_media(url: &str) -> bool {
    let path = url
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(url)
        .split(['?', '#'])
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    MEDIA_SUFFIXES.iter().any(|suffix| path.ends_with(suffix))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn challenge_html_path_on_same_host_is_opened() {
        assert_eq!(
            page_to_open("vcloud.my", "https://vcloud.my/uthhuboxjuitfuz"),
            "https://vcloud.my/uthhuboxjuitfuz"
        );
        assert_eq!(
            page_to_open(
                "ww5.m4ufree.lat",
                "https://ww5.m4ufree.lat/search/spider-man-brand-new-day"
            ),
            "https://ww5.m4ufree.lat/search/spider-man-brand-new-day"
        );
    }

    #[test]
    fn media_and_foreign_hosts_fall_back_to_front_page() {
        assert_eq!(
            page_to_open("cdn.test", "https://cdn.test/play.m3u8?tok=1"),
            "https://cdn.test/"
        );
        assert_eq!(
            page_to_open("a.test", "https://other.test/challenge"),
            "https://a.test/"
        );
        assert_eq!(page_to_open("a.test", "ftp://a.test/x"), "https://a.test/");
    }

    #[test]
    fn subdomain_of_allowed_host_keeps_challenge_url() {
        assert_eq!(
            page_to_open("site.test", "https://cdn.site.test/path"),
            "https://cdn.site.test/path"
        );
    }
}
