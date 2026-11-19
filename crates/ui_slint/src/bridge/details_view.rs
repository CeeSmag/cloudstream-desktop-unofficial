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
            season: row.season,
            index: i32::try_from(index).unwrap_or(i32::MAX),
        })
        .collect();
    ui.set_episodes(ModelRc::new(VecModel::from(rows)));
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
        }
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
