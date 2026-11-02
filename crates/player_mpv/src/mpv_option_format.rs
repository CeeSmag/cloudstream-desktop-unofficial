use player_api::{PlaybackRequest, SubtitleSource};

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

/// Builds the value of the `start` option: seconds with one decimal, or `none` to start at 0.
pub(crate) fn start_option(request: &PlaybackRequest) -> String {
    if request.start_position_ms <= 0 {
        return "none".to_owned();
    }
    format!("{:.1}", request.start_position_ms as f64 / 1000.0)
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
}
