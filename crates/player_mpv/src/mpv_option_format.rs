use player_api::{PlaybackRequest, PlayerError, SubtitleSource};

use crate::mpv_api::MpvHandle;

/// Default User-Agent restored when a stream does not specify its own.
const DEFAULT_USER_AGENT: &str = "libmpv";

/// Encodes one string for mpv's list options using the length-prefixed `%len%value` form,
/// which keeps commas and colons inside the value intact.
fn length_prefixed(value: &str) -> String {
    format!("%{}%{value}", value.len())
}

/// Builds the value of `http-header-fields` from (name, value) pairs.
pub(crate) fn header_fields(headers: &[(String, String)]) -> String {
    headers
        .iter()
        .map(|(name, value)| length_prefixed(&format!("{name}: {value}")))
        .collect::<Vec<_>>()
        .join(",")
}

/// Extracts a custom `User-Agent` from `headers`, or falls back to `DEFAULT_USER_AGENT`.
pub(crate) fn user_agent_option(headers: &[(String, String)]) -> &str {
    headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("user-agent"))
        .map(|(_, value)| value.as_str())
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_USER_AGENT)
}

/// Builds `demuxer-lavf-o` options for ClearKey CENC decryption (`decryption_key` for MP4/MOV and
/// `cenc_decryption_key` for DASH `.mpd`), or empty string to clear keys from previous streams.
pub(crate) fn lavf_options(request: &PlaybackRequest) -> String {
    match request.clear_key.as_deref().filter(|key| !key.is_empty()) {
        Some(key) => format!("decryption_key={key},cenc_decryption_key={key}"),
        None => String::new(),
    }
}

/// Builds the value of the `start` option: seconds with one decimal, or `none` to start at 0.
pub(crate) fn start_option(request: &PlaybackRequest) -> String {
    if request.start_position_ms <= 0 {
        return "none".to_owned();
    }
    format!("{:.1}", request.start_position_ms as f64 / 1000.0)
}

/// Applies per-stream request properties (`http-header-fields`, `user-agent`, `referrer`,
/// `demuxer-lavf-o`, `force-media-title`, `start`) before `loadfile`.
pub(crate) fn apply_request_options(
    handle: &MpvHandle,
    request: &PlaybackRequest,
) -> Result<(), PlayerError> {
    handle.set_property("http-header-fields", &header_fields(&request.headers))?;
    handle.set_property("user-agent", user_agent_option(&request.headers))?;
    handle.set_property("referrer", &request.referer)?;
    handle.set_property("demuxer-lavf-o", &lavf_options(request))?;
    handle.set_property("force-media-title", &request.title)?;
    handle.set_property("start", &start_option(request))
}

/// Returns the subtitle label shown in mpv's track list.
pub(crate) fn subtitle_label(subtitle: &SubtitleSource) -> &str {
    if subtitle.language.is_empty() {
        "Subtitles"
    } else {
        &subtitle.language
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_fields_use_length_prefixes_so_commas_survive() {
        let headers = vec![
            ("Referer".to_owned(), "https://a.test/x,y".to_owned()),
            ("User-Agent".to_owned(), "UA".to_owned()),
        ];
        assert_eq!(
            header_fields(&headers),
            "%27%Referer: https://a.test/x,y,%14%User-Agent: UA"
        );
    }

    #[test]
    fn header_fields_empty_for_no_headers() {
        assert_eq!(header_fields(&[]), "");
    }

    #[test]
    fn start_option_is_none_at_zero_and_seconds_otherwise() {
        let at = |ms| PlaybackRequest {
            start_position_ms: ms,
            ..PlaybackRequest::default()
        };
        assert_eq!(start_option(&at(0)), "none");
        assert_eq!(start_option(&at(-5)), "none");
        assert_eq!(start_option(&at(12_340)), "12.3");
    }

    #[test]
    fn subtitle_label_falls_back_when_language_missing() {
        assert_eq!(subtitle_label(&SubtitleSource::default()), "Subtitles");
        let english = SubtitleSource {
            url: "u".into(),
            language: "English".into(),
        };
        assert_eq!(subtitle_label(&english), "English");
    }

    #[test]
    fn lavf_options_sets_both_mp4_and_dash_cenc_decryption_keys() {
        let unlocked = PlaybackRequest::default();
        assert_eq!(lavf_options(&unlocked), "");

        let drm = PlaybackRequest {
            clear_key: Some("567c863bc12eb74788ea74888c042e1b".into()),
            ..PlaybackRequest::default()
        };
        assert_eq!(
            lavf_options(&drm),
            "decryption_key=567c863bc12eb74788ea74888c042e1b,cenc_decryption_key=567c863bc12eb74788ea74888c042e1b"
        );
    }

    #[test]
    fn user_agent_option_finds_case_insensitive_header_or_defaults() {
        assert_eq!(user_agent_option(&[]), "libmpv");
        let headers = vec![("user-agent".to_owned(), "Mozilla/5.0".to_owned())];
        assert_eq!(user_agent_option(&headers), "Mozilla/5.0");
    }
}
