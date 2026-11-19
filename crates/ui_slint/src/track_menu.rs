//! The player's Audio and Subtitles menus: their rows, in what order, with which labels.
//!
//! Ported from the old app (`trackMenu.ts`, `trackLabels.ts`): subtitles in the viewer's preferred
//! languages come first, in the order they were chosen, with a language tag; the rest keep the
//! stream's order. Several tracks with the same name ("English" nine times) are numbered so the
//! viewer can tell them apart, and a pick goes by the track's id, not its label.

use player_api::{Language, MediaTrack, TrackKind};

use crate::source_menu::abbreviation;

/// One row of a track menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TrackRow {
    /// The player's id of the track; [`OFF_ID`] for "Off".
    pub(crate) id: i64,
    pub(crate) label: String,
    /// The preferred language it is in (`EN`); empty for the rest.
    pub(crate) flag: String,
    pub(crate) selected: bool,
}

/// The id of the subtitles menu's "Off" row.
pub(crate) const OFF_ID: i64 = -1;

/// The rows for the tracks of `kind`; the subtitles menu starts with "Off" and puts the
/// `preferred` languages first.
pub(crate) fn track_rows(
    tracks: &[MediaTrack],
    kind: TrackKind,
    preferred: &[Language],
) -> Vec<TrackRow> {
    let of_kind: Vec<&MediaTrack> = tracks.iter().filter(|track| track.kind == kind).collect();
    let labels = number_duplicates(of_kind.iter().map(|track| track.label()).collect());
    let mut rows: Vec<(usize, TrackRow)> = of_kind
        .iter()
        .zip(labels)
        .map(|(track, label)| {
            let rank = match kind {
                TrackKind::Subtitle => preferred.iter().position(|l| track.is_language(l)),
                TrackKind::Audio => None,
            };
            let flag = rank
                .and_then(|rank| preferred.get(rank))
                .map(|language| abbreviation(language.name))
                .unwrap_or_default();
            let row = TrackRow {
                id: track.id,
                label,
                flag,
                selected: track.selected,
            };
            (rank.unwrap_or(usize::MAX), row)
        })
        .collect();
    // Stable: tracks of the same rank keep the stream's order.
    rows.sort_by_key(|(rank, _)| *rank);
    let mut shown: Vec<TrackRow> = rows.into_iter().map(|(_, row)| row).collect();
    if kind == TrackKind::Subtitle {
        let none_on = !shown.iter().any(|row| row.selected);
        shown.insert(0, off_row(none_on));
    }
    shown
}

fn off_row(selected: bool) -> TrackRow {
    TrackRow {
        id: OFF_ID,
        label: "Off".to_owned(),
        flag: String::new(),
        selected,
    }
}

/// Numbers the second and later of labels that occur more than once: `English 1`, `English 2`.
pub(crate) fn number_duplicates(labels: Vec<String>) -> Vec<String> {
    let total = |label: &str| labels.iter().filter(|other| *other == label).count();
    let mut seen: Vec<(String, usize)> = Vec::new();
    labels
        .iter()
        .map(|label| {
            if total(label) < 2 {
                return label.clone();
            }
            let position = match seen.iter_mut().find(|(name, _)| name == label) {
                Some((_, count)) => {
                    *count += 1;
                    *count
                }
                None => {
                    seen.push((label.clone(), 1));
                    1
                }
            };
            format!("{label} {position}")
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use player_api::find_language;

    use super::*;

    fn track(id: i64, kind: TrackKind, language: &str, selected: bool) -> MediaTrack {
        MediaTrack {
            id,
            kind,
            language: language.to_owned(),
            title: String::new(),
            selected,
            external: false,
        }
    }

    fn preferred(names: &[&str]) -> Vec<Language> {
        names
            .iter()
            .filter_map(|name| find_language(name))
            .collect()
    }

    #[test]
    fn track_rows_subtitles_put_preferred_languages_first_in_their_order() {
        let tracks = [
            track(1, TrackKind::Subtitle, "fre", false),
            track(2, TrackKind::Subtitle, "eng", false),
            track(3, TrackKind::Subtitle, "ara", true),
            track(4, TrackKind::Audio, "eng", true),
        ];
        let rows = track_rows(
            &tracks,
            TrackKind::Subtitle,
            &preferred(&["Arabic", "English"]),
        );
        let ids: Vec<i64> = rows.iter().map(|row| row.id).collect();
        assert_eq!(ids, [OFF_ID, 3, 2, 1]);
        let flags: Vec<&str> = rows.iter().map(|row| row.flag.as_str()).collect();
        assert_eq!(flags, ["", "AR", "EN", ""]);
        assert!(rows.get(1).is_some_and(|row| row.selected));
        assert!(rows.first().is_some_and(|off| !off.selected));
    }

    #[test]
    fn track_rows_off_is_selected_when_no_subtitle_shows() {
        let tracks = [track(1, TrackKind::Subtitle, "eng", false)];
        let rows = track_rows(&tracks, TrackKind::Subtitle, &[]);
        assert!(rows
            .first()
            .is_some_and(|off| off.selected && off.label == "Off"));
    }

    #[test]
    fn track_rows_audio_keeps_stream_order_and_numbers_duplicates() {
        let tracks = [
            track(7, TrackKind::Audio, "eng", false),
            track(8, TrackKind::Audio, "jpn", true),
            track(9, TrackKind::Audio, "eng", false),
        ];
        let rows = track_rows(&tracks, TrackKind::Audio, &preferred(&["Japanese"]));
        let labels: Vec<&str> = rows.iter().map(|row| row.label.as_str()).collect();
        assert_eq!(labels, ["English 1", "Japanese", "English 2"]);
        assert!(rows.iter().all(|row| row.flag.is_empty()));
    }

    #[test]
    fn number_duplicates_leaves_unique_labels_alone() {
        let labels = vec![
            "A".to_owned(),
            "B".to_owned(),
            "A".to_owned(),
            "A".to_owned(),
        ];
        assert_eq!(number_duplicates(labels), ["A 1", "B", "A 2", "A 3"]);
    }
}
