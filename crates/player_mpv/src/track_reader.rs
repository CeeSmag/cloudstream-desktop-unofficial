use player_api::{MediaTrack, TrackKind};

/// Builds the track list from mpv's `track-list/N/...` properties. `get` returns a property as text.
pub(crate) fn read_tracks(get: impl Fn(&str) -> Option<String>) -> Vec<MediaTrack> {
    let count: usize = get("track-list/count")
        .and_then(|text| text.parse().ok())
        .unwrap_or(0);
    (0..count)
        .filter_map(|index| read_track(&get, index))
        .collect()
}

/// Whether the file has a real video picture (cover art attached to audio does not count).
pub(crate) fn has_video_track(get: impl Fn(&str) -> Option<String>) -> bool {
    let count: usize = get("track-list/count")
        .and_then(|text| text.parse().ok())
        .unwrap_or(0);
    (0..count).any(|index| {
        let field = |name: &str| get(&format!("track-list/{index}/{name}"));
        field("type").as_deref() == Some("video") && field("albumart").as_deref() != Some("yes")
    })
}

fn read_track(get: &impl Fn(&str) -> Option<String>, index: usize) -> Option<MediaTrack> {
    let field = |name: &str| get(&format!("track-list/{index}/{name}"));
    let kind = match field("type")?.as_str() {
        "audio" => TrackKind::Audio,
        "sub" => TrackKind::Subtitle,
        _ => return None,
    };
    Some(MediaTrack {
        id: field("id")?.parse().ok()?,
        kind,
        language: field("lang").unwrap_or_default(),
        title: field("title").unwrap_or_default(),
        selected: field("selected").as_deref() == Some("yes"),
        external: field("external").as_deref() == Some("yes"),
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn properties(entries: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = entries
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        move |name| map.get(name).cloned()
    }

    #[test]
    fn read_tracks_keeps_audio_and_subtitles_and_skips_video() {
        let get = properties(&[
            ("track-list/count", "4"),
            ("track-list/0/type", "video"),
            ("track-list/0/id", "1"),
            ("track-list/1/type", "audio"),
            ("track-list/1/id", "1"),
            ("track-list/1/lang", "eng"),
            ("track-list/1/selected", "yes"),
            ("track-list/2/type", "audio"),
            ("track-list/2/id", "2"),
            ("track-list/2/title", "Commentary"),
            ("track-list/3/type", "sub"),
            ("track-list/3/id", "1"),
            ("track-list/3/external", "yes"),
        ]);
        let tracks = read_tracks(get);
        assert_eq!(tracks.len(), 3);
        assert!(tracks
            .first()
            .is_some_and(|t| t.selected && t.kind == TrackKind::Audio && t.language == "eng"));
        assert!(tracks
            .get(1)
            .is_some_and(|t| !t.selected && t.title == "Commentary"));
        assert!(tracks
            .get(2)
            .is_some_and(|t| t.kind == TrackKind::Subtitle && t.external));
    }

    #[test]
    fn has_video_track_ignores_cover_art_and_audio_only_files() {
        let with_video = properties(&[
            ("track-list/count", "2"),
            ("track-list/0/type", "audio"),
            ("track-list/1/type", "video"),
        ]);
        assert!(has_video_track(with_video));
        let audio_only = properties(&[("track-list/count", "1"), ("track-list/0/type", "audio")]);
        assert!(!has_video_track(audio_only));
        let cover = properties(&[
            ("track-list/count", "1"),
            ("track-list/0/type", "video"),
            ("track-list/0/albumart", "yes"),
        ]);
        assert!(!has_video_track(cover));
    }

    #[test]
    fn read_tracks_is_empty_when_mpv_has_no_track_list_yet() {
        assert!(read_tracks(properties(&[])).is_empty());
    }

    #[test]
    fn read_tracks_skips_entries_with_unreadable_ids() {
        let get = properties(&[
            ("track-list/count", "1"),
            ("track-list/0/type", "audio"),
            ("track-list/0/id", "x"),
        ]);
        assert!(read_tracks(get).is_empty());
    }
}
