//! The title page's episode list (one season at a time) and its artwork.

use slint::{ModelRc, VecModel};

use crate::ui_events::EpisodeRow;
use crate::{CloudStream, EpisodeItem};

use super::bridge_context::BridgeContext;
use super::poster_hydrate::request_artwork;

/// Shows the episodes of the chosen season (all when no season is chosen).
pub(crate) fn show_episode_list(ctx: &BridgeContext, ui: &CloudStream) {
    let season = ui.get_season_filter();
    let rows: Vec<EpisodeItem> = shown_episodes(&ctx.session.borrow().episodes, season)
        .into_iter()
        .map(|(index, row)| EpisodeItem {
            name: row.name.clone().into(),
            code: row.code.clone().into(),
            data: row.data.clone().into(),
            watched: row.watched,
            progress: row.progress,
            description: row.description.clone().into(),
            season: row.season,
            index: i32::try_from(index).unwrap_or(i32::MAX),
        })
        .collect();
    ui.set_episodes(ModelRc::new(VecModel::from(rows)));
    let label = play_label(&ctx.session.borrow().episodes, season);
    ui.set_play_label(label.into());
}

/// The season the title page opens on: the one of the episode the viewer left off at, so it is
/// in view (like the old title page); -1 (every season) otherwise.
pub(crate) fn resume_season(episodes: &[EpisodeRow], resume_key: Option<&str>) -> i32 {
    let Some(key) = resume_key else {
        return -1;
    };
    let seasons: std::collections::BTreeSet<i32> = episodes.iter().map(|row| row.season).collect();
    if seasons.len() < 2 {
        return -1;
    }
    episodes
        .iter()
        .find(|row| row.data == key)
        .map_or(-1, |row| row.season.max(-1))
}

/// The episode Play starts: the first of the shown season (by season, then episode number).
pub(crate) fn first_episode(episodes: &[EpisodeRow], season: i32) -> Option<usize> {
    shown_episodes(episodes, season)
        .into_iter()
        .min_by_key(|(index, row)| (row.season, row.episode.unwrap_or(i32::MAX), *index))
        .map(|(index, _)| index)
}

/// "Play S1 E1" for a show, "Play" for a movie.
pub(crate) fn play_label(episodes: &[EpisodeRow], season: i32) -> String {
    let first = first_episode(episodes, season).and_then(|index| episodes.get(index));
    match first.map(|row| (row.season, row.episode)) {
        Some((season, Some(number))) if season >= 0 => format!("Play S{season} E{number}"),
        Some((_, Some(number))) => format!("Play E{number}"),
        _ => "Play".to_owned(),
    }
}

/// The episodes of `season` (-1 = all) with their place in the full list.
pub(crate) fn shown_episodes(episodes: &[EpisodeRow], season: i32) -> Vec<(usize, &EpisodeRow)> {
    episodes
        .iter()
        .enumerate()
        .filter(|(_, row)| season < 0 || row.season == season)
        .collect()
}

/// Loads the poster of the open title page (shown beside the title and blurred behind it).
pub(crate) fn show_details_artwork(ctx: &BridgeContext, ui: &CloudStream) {
    ui.set_details_poster(slint::Image::default());
    let Some(url) = ctx.session.borrow().details.poster_url.clone() else {
        return;
    };
    let title = ui.get_details_title();
    request_artwork(ctx, &url, move |ui, image| {
        if ui.get_details_title() == title {
            ui.set_details_poster(image);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(season: i32) -> EpisodeRow {
        EpisodeRow {
            name: String::new(),
            code: String::new(),
            data: String::new(),
            watched: false,
            season,
            episode: None,
            ..EpisodeRow::default()
        }
    }

    fn numbered(season: i32, episode: i32) -> EpisodeRow {
        EpisodeRow {
            season,
            episode: Some(episode),
            ..EpisodeRow::default()
        }
    }

    #[test]
    fn resume_season_opens_where_the_viewer_left_off() {
        let mut episodes = vec![numbered(1, 1), numbered(2, 1)];
        if let Some(row) = episodes.get_mut(1) {
            row.data = "s2e1".into();
        }
        assert_eq!(resume_season(&episodes, Some("s2e1")), 2);
        assert_eq!(resume_season(&episodes, Some("gone")), -1);
        assert_eq!(resume_season(&episodes, None), -1);
        let one_season = [numbered(1, 1)];
        assert_eq!(
            resume_season(&one_season, Some("")),
            -1,
            "nothing to choose"
        );
    }

    #[test]
    fn first_episode_and_play_label_follow_the_shown_season() {
        let episodes = [numbered(2, 1), numbered(1, 2), numbered(1, 1)];
        assert_eq!(first_episode(&episodes, -1), Some(2));
        assert_eq!(play_label(&episodes, -1), "Play S1 E1");
        assert_eq!(first_episode(&episodes, 2), Some(0));
        assert_eq!(play_label(&episodes, 2), "Play S2 E1");
        assert_eq!(play_label(&[], -1), "Play", "a movie");
        assert_eq!(play_label(&[numbered(-1, 4)], -1), "Play E4");
    }

    #[test]
    fn shown_episodes_keeps_original_indexes() {
        let episodes = [row(1), row(2), row(1)];
        let indexes: Vec<usize> = shown_episodes(&episodes, 1)
            .into_iter()
            .map(|(index, _)| index)
            .collect();
        assert_eq!(indexes, [0, 2]);
        assert_eq!(shown_episodes(&episodes, -1).len(), 3);
    }
}
