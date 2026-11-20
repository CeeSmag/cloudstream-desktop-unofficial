//! Season grouping, sorting, range chunking, and responsive grid math for the Details episode list.

use std::collections::BTreeSet;

use crate::ui_events::EpisodeRow;

/// Episodes shown per page; longer seasons split into 20-episode range pills.
pub(crate) const CHUNK_SIZE: usize = 20;

/// 20-episode sub-ranges grouped under one 100-episode macro pill.
pub(crate) const MACRO_CHUNK_COUNT: usize = 5;

/// Above this many seasons the picker switches from pills to a dropdown.
pub(crate) const MAX_SEASON_TABS: usize = 6;

/// Episodes without a known season belong to Season 1; season 0 is Specials.
pub(crate) fn normalized_season(raw: i32) -> i32 {
    if raw < 0 {
        1
    } else {
        raw
    }
}

/// Unique season numbers: regular seasons ascending, then specials (`<= 0`) ascending.
pub(crate) fn season_numbers(episodes: &[EpisodeRow]) -> Vec<i32> {
    let unique: BTreeSet<i32> = episodes
        .iter()
        .map(|row| normalized_season(row.season))
        .collect();
    let regular = unique.iter().copied().filter(|season| *season > 0);
    let specials = unique.iter().copied().filter(|season| *season <= 0);
    regular.chain(specials).collect()
}

/// Display label of a season tab (`"Season 1"`, or `"Specials"` for season 0 and below).
pub(crate) fn season_label(season: i32) -> String {
    if season <= 0 {
        "Specials".to_owned()
    } else {
        format!("Season {season}")
    }
}

/// Display labels of every season present in `episodes`.
pub(crate) fn season_labels(episodes: &[EpisodeRow]) -> Vec<String> {
    season_numbers(episodes)
        .into_iter()
        .map(season_label)
        .collect()
}

/// Parses a label produced by [`season_label`] back into its season number.
pub(crate) fn parse_season_label(label: &str) -> i32 {
    if label == "Specials" {
        return 0;
    }
    label
        .strip_prefix("Season ")
        .and_then(|number| number.parse().ok())
        .unwrap_or(1)
}

/// The season Details opens on: the resumed episode's season, else the first season.
pub(crate) fn initial_season(episodes: &[EpisodeRow], resume_key: Option<&str>) -> i32 {
    if let Some(key) = resume_key {
        if let Some(found) = episodes.iter().find(|row| row.data == key) {
            return normalized_season(found.season);
        }
    }
    season_numbers(episodes).first().copied().unwrap_or(1)
}

/// Episodes of `season` paired with their index in the provider's full list, sorted by number.
pub(crate) fn season_episodes(
    episodes: &[EpisodeRow],
    season: i32,
    ascending: bool,
) -> Vec<(usize, &EpisodeRow)> {
    let mut rows: Vec<(usize, &EpisodeRow)> = episodes
        .iter()
        .enumerate()
        .filter(|(_, row)| normalized_season(row.season) == season)
        .collect();
    rows.sort_by_key(|(index, row)| (row.episode.unwrap_or(i32::MAX), *index));
    if !ascending {
        rows.reverse();
    }
    rows
}

/// The lowest-numbered episode of `season` (what the Play button starts regardless of sort order).
pub(crate) fn first_episode_in_season(episodes: &[EpisodeRow], season: i32) -> Option<usize> {
    season_episodes(episodes, season, true)
        .first()
        .map(|(index, _)| *index)
}

/// Number of [`CHUNK_SIZE`]-episode ranges needed for `total` episodes.
pub(crate) fn chunk_count(total: usize) -> usize {
    total.div_ceil(CHUNK_SIZE)
}

/// Slice of `sorted` belonging to range `chunk` (clamped to the last valid range).
pub(crate) fn chunk_slice<'a>(
    sorted: &'a [(usize, &'a EpisodeRow)],
    chunk: usize,
) -> &'a [(usize, &'a EpisodeRow)] {
    let total_chunks = chunk_count(sorted.len());
    if total_chunks == 0 {
        return &[];
    }
    let clamped = chunk.min(total_chunks - 1);
    let start = clamped * CHUNK_SIZE;
    let end = (start + CHUNK_SIZE).min(sorted.len());
    sorted.get(start..end).unwrap_or(&[])
}

/// Range pill label such as `"1–20"`, using episode numbers when available.
pub(crate) fn chunk_label(slice: &[(usize, &EpisodeRow)], chunk_index: usize) -> String {
    let first = slice
        .first()
        .and_then(|(_, row)| row.episode)
        .unwrap_or_else(|| i32::try_from(chunk_index * CHUNK_SIZE + 1).unwrap_or(1));
    let last = slice
        .last()
        .and_then(|(_, row)| row.episode)
        .unwrap_or_else(|| i32::try_from(chunk_index * CHUNK_SIZE + slice.len()).unwrap_or(first));
    format!("{first}–{last}")
}

/// 100-episode macro-group labels (`"1–100"`, `"101–200"`); empty when `<= 1` macro group.
pub(crate) fn macro_labels(sorted: &[(usize, &EpisodeRow)]) -> Vec<String> {
    let chunks = chunk_count(sorted.len());
    let macros = chunks.div_ceil(MACRO_CHUNK_COUNT);
    if macros <= 1 {
        return Vec::new();
    }
    (0..macros)
        .map(|macro_idx| {
            let first_chunk = macro_idx * MACRO_CHUNK_COUNT;
            let last_chunk = ((macro_idx + 1) * MACRO_CHUNK_COUNT).min(chunks) - 1;
            let first_slice = chunk_slice(sorted, first_chunk);
            let last_slice = chunk_slice(sorted, last_chunk);
            let first = first_slice
                .first()
                .and_then(|(_, row)| row.episode)
                .unwrap_or_else(|| i32::try_from(first_chunk * CHUNK_SIZE + 1).unwrap_or(1));
            let last = last_slice
                .last()
                .and_then(|(_, row)| row.episode)
                .unwrap_or_else(|| {
                    i32::try_from(last_chunk * CHUNK_SIZE + last_slice.len()).unwrap_or(first)
                });
            format!("{first}–{last}")
        })
        .collect()
}

/// 20-episode sub-range labels inside `macro_index`; empty when `<= 1` sub-range.
pub(crate) fn range_labels_for_macro(
    sorted: &[(usize, &EpisodeRow)],
    macro_index: usize,
) -> Vec<String> {
    let chunks = chunk_count(sorted.len());
    let start_chunk = macro_index * MACRO_CHUNK_COUNT;
    let end_chunk = (start_chunk + MACRO_CHUNK_COUNT).min(chunks);
    if end_chunk.saturating_sub(start_chunk) <= 1 {
        return Vec::new();
    }
    (start_chunk..end_chunk)
        .map(|chunk| chunk_label(chunk_slice(sorted, chunk), chunk))
        .collect()
}

/// Range index containing `key`, so Details opens directly on the resumed episode's page.
pub(crate) fn chunk_for_key(sorted: &[(usize, &EpisodeRow)], key: &str) -> Option<usize> {
    sorted
        .iter()
        .position(|(_, row)| row.data == key)
        .map(|pos| pos / CHUNK_SIZE)
}

/// Responsive episode card column count (`1..=4`) matching CSS `minmax(320px, 1fr)` with 16px gap.
pub(crate) fn episode_columns_for_width(logical_width: f32) -> i32 {
    let avail = (logical_width - 86.0).clamp(320.0, 1520.0);
    (((avail + 16.0) / 336.0).floor() as i32).clamp(1, 4)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ep(season: i32, number: Option<i32>, key: &str) -> EpisodeRow {
        EpisodeRow {
            season,
            episode: number,
            data: key.to_owned(),
            ..EpisodeRow::default()
        }
    }

    #[test]
    fn seasons_order_regular_first_and_specials_last() {
        let episodes = [
            ep(0, Some(1), "sp1"),
            ep(2, Some(1), "s2e1"),
            ep(-1, Some(1), "s1e1"),
            ep(1, Some(2), "s1e2"),
        ];
        assert_eq!(season_numbers(&episodes), [1, 2, 0]);
        assert_eq!(
            season_labels(&episodes),
            ["Season 1", "Season 2", "Specials"]
        );
        assert_eq!(parse_season_label("Season 2"), 2);
        assert_eq!(parse_season_label("Specials"), 0);
    }

    #[test]
    fn initial_season_picks_resumed_else_first() {
        let episodes = [ep(1, Some(1), "s1e1"), ep(3, Some(1), "s3e1")];
        assert_eq!(initial_season(&episodes, Some("s3e1")), 3);
        assert_eq!(initial_season(&episodes, Some("missing")), 1);
        assert_eq!(initial_season(&episodes, None), 1);
    }

    #[test]
    fn sort_and_first_episode_handle_ascending_and_descending() {
        let episodes = [
            ep(1, Some(3), "e3"),
            ep(1, Some(1), "e1"),
            ep(1, Some(2), "e2"),
        ];
        let asc: Vec<usize> = season_episodes(&episodes, 1, true)
            .into_iter()
            .map(|(i, _)| i)
            .collect();
        let desc: Vec<usize> = season_episodes(&episodes, 1, false)
            .into_iter()
            .map(|(i, _)| i)
            .collect();
        assert_eq!(asc, [1, 2, 0]);
        assert_eq!(desc, [0, 2, 1]);
        assert_eq!(first_episode_in_season(&episodes, 1), Some(1));
    }

    #[test]
    fn chunking_and_macro_groups_paginate_long_seasons() {
        let episodes: Vec<EpisodeRow> = (1..=125)
            .map(|n| ep(1, Some(n), &format!("ep{n}")))
            .collect();
        let sorted = season_episodes(&episodes, 1, true);
        assert_eq!(chunk_count(sorted.len()), 7);
        assert_eq!(macro_labels(&sorted), ["1–100", "101–125"]);
        assert_eq!(
            range_labels_for_macro(&sorted, 0),
            ["1–20", "21–40", "41–60", "61–80", "81–100"]
        );
        assert_eq!(range_labels_for_macro(&sorted, 1), ["101–120", "121–125"]);
        assert_eq!(chunk_for_key(&sorted, "ep45"), Some(2));
        assert_eq!(chunk_slice(&sorted, 6).len(), 5);
    }

    #[test]
    fn episode_columns_scale_across_window_widths() {
        assert_eq!(episode_columns_for_width(600.0), 1);
        assert_eq!(episode_columns_for_width(960.0), 2);
        assert_eq!(episode_columns_for_width(1280.0), 3);
        assert_eq!(episode_columns_for_width(1600.0), 4);
        assert_eq!(episode_columns_for_width(2400.0), 4);
    }
}
