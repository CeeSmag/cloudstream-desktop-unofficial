use core_model::PlayableLink;

use crate::is_audio_only_url;

/// Longest label kept as the link's own name; longer names are shortened to the hoster.
const MAX_LABEL_CHARS: usize = 48;
/// Longest container hint taken from a trailing `(...)` in a link name.
const MAX_HINT_CHARS: usize = 8;

/// `1080p - Server 1`: quality and the link's own name (or its source when it has no name).
pub fn describe_link(link: &PlayableLink) -> String {
    let name = if link.name.is_empty() {
        &link.source
    } else {
        &link.name
    };
    format!("{} - {name}", quality_text(link))
}

/// `1080p - Server (mp4)`: the hoster name when the link name is a long file name, plus the
/// container hint in brackets when the name ends with one. Audio-only streams are marked.
pub fn source_label(link: &PlayableLink) -> String {
    let label = base_label(link);
    if is_audio_only_url(&link.url) {
        format!("{label} \u{00B7} audio only")
    } else {
        label
    }
}

fn quality_text(link: &PlayableLink) -> String {
    if link.quality > 0 {
        format!("{}p", link.quality)
    } else {
        "auto".to_owned()
    }
}

fn base_label(link: &PlayableLink) -> String {
    let description = describe_link(link);
    if description.chars().count() <= MAX_LABEL_CHARS || link.source.is_empty() {
        return description;
    }
    let quality = quality_text(link);
    match format_hint(&link.name) {
        Some(hint) => format!("{quality} - {} ({hint})", link.source),
        None => format!("{quality} - {}", link.source),
    }
}

/// The text inside a trailing `(...)`, for example `mp4` in `Some Title (mp4)`.
fn format_hint(name: &str) -> Option<&str> {
    let inner = name.trim_end().strip_suffix(')')?;
    let (_, hint) = inner.rsplit_once('(')?;
    (!hint.is_empty() && hint.len() <= MAX_HINT_CHARS).then_some(hint)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describe_shows_quality_and_name() {
        let link = PlayableLink {
            name: "Server 1".into(),
            quality: 1080,
            ..PlayableLink::default()
        };
        assert_eq!(describe_link(&link), "1080p - Server 1");
    }

    #[test]
    fn describe_falls_back_to_auto_and_source() {
        let link = PlayableLink {
            source: "Hoster".into(),
            ..PlayableLink::default()
        };
        assert_eq!(describe_link(&link), "auto - Hoster");
    }

    #[test]
    fn source_label_marks_audio_only_streams() {
        let link = PlayableLink {
            url: "https://h/a.mp3".into(),
            name: "Talk (mp3)".into(),
            quality: 400,
            ..PlayableLink::default()
        };
        assert!(source_label(&link).ends_with("audio only"));
        let video = PlayableLink {
            url: "https://h/a.mp4".into(),
            name: "Talk (mp4)".into(),
            ..PlayableLink::default()
        };
        assert!(!source_label(&video).contains("audio only"));
    }

    #[test]
    fn format_hint_reads_a_short_trailing_bracket_only() {
        assert_eq!(format_hint("Title (mp3)"), Some("mp3"));
        assert_eq!(format_hint("Title"), None);
        assert_eq!(format_hint("Title (a very long explanation)"), None);
    }

    #[test]
    fn source_label_shortens_long_file_names_to_the_hoster() {
        let link = PlayableLink {
            source: "Internet Archive".into(),
            name: "Women_s_History_Month_Nancy_Kopp_Interview_March_31_2023 (mp4)".into(),
            quality: 400,
            ..PlayableLink::default()
        };
        assert_eq!(source_label(&link), "400p - Internet Archive (mp4)");
    }
}
