//! The title page's episode list (selected season, sort order, 20-episode ranges, and thumbnails).

use slint::{ModelRc, VecModel};

use crate::episode_groups::{
    chunk_count, chunk_slice, first_episode_in_season, macro_labels, range_labels_for_macro,
    season_episodes, season_label, MACRO_CHUNK_COUNT,
};
use crate::posters::string_model;
use crate::ui_events::EpisodeRow;
use crate::{CloudStream, EpisodeItem};

use super::bridge_context::BridgeContext;
use super::details_artwork::hydrate_episode_thumbnails;

/// Rebuilds the visible episode cards, range pills, and Play/Resume button visibility.
pub(crate) fn show_episode_list(ctx: &BridgeContext, ui: &CloudStream) {
    let season = ui.get_season_filter();
    let (ascending, chunk, resume_key, episodes) = {
        let session = ctx.session.borrow();
        (
            !session.details.episodes_descending,
            session.details.episode_chunk,
            session.details.resume_media_key.clone(),
            session.episodes.clone(),
        )
    };
    let sorted = season_episodes(&episodes, season, ascending);
    let total_chunks = chunk_count(sorted.len());
    let clamped_chunk = chunk.min(total_chunks.saturating_sub(1));
    ctx.session.borrow_mut().details.episode_chunk = clamped_chunk;
    let active_macro = clamped_chunk / MACRO_CHUNK_COUNT;
    let active_range = clamped_chunk % MACRO_CHUNK_COUNT;

    let rows: Vec<EpisodeItem> = chunk_slice(&sorted, clamped_chunk)
        .iter()
        .map(|(index, row)| to_episode_item(*index, row))
        .collect();

    ui.set_episodes(ModelRc::new(VecModel::from(rows)));
    ui.set_episode_total_count(i32::try_from(sorted.len()).unwrap_or(i32::MAX));
    ui.set_episodes_ascending(ascending);
    ui.set_active_season_label(season_label(season).into());
    ui.set_episode_macro_labels(string_model(macro_labels(&sorted)));
    ui.set_episode_active_macro(i32::try_from(active_macro).unwrap_or(0));
    ui.set_episode_range_labels(string_model(range_labels_for_macro(&sorted, active_macro)));
    ui.set_episode_active_range(i32::try_from(active_range).unwrap_or(0));
    update_play_buttons(ui, &episodes, season, resume_key.as_deref());
    hydrate_episode_thumbnails(ctx, ui);
}

fn to_episode_item(index: usize, row: &EpisodeRow) -> EpisodeItem {
    EpisodeItem {
        name: row.name.clone().into(),
        episode_label: row.episode_label.clone().into(),
        data: row.data.clone().into(),
        watched: row.watched,
        progress: row.progress,
        description: row.description.clone().into(),
        index: i32::try_from(index).unwrap_or(i32::MAX),
        poster: row.poster_url.clone().unwrap_or_default().into(),
        image: slint::Image::default(),
    }
}

fn update_play_buttons(
    ui: &CloudStream,
    episodes: &[EpisodeRow],
    season: i32,
    resume_key: Option<&str>,
) {
    let first = first_episode_in_season(episodes, season);
    ui.set_play_label(play_label(episodes, season).into());
    let first_key = first
        .and_then(|idx| episodes.get(idx))
        .map(|ep| ep.data.as_str());
    let show_secondary = match (resume_key, first_key) {
        (Some(resume), Some(first_ep)) if !resume.is_empty() => resume != first_ep,
        _ => false,
    };
    ui.set_show_secondary_play(show_secondary);
}

/// `"Play S1 E1"` for a show, `"Play"` for a movie.
pub(crate) fn play_label(episodes: &[EpisodeRow], season: i32) -> String {
    let first = first_episode_in_season(episodes, season).and_then(|index| episodes.get(index));
    match first {
        Some(row) => match row.episode {
            Some(number) if season > 0 => format!("Play S{season} E{number}"),
            Some(number) => format!("Play E{number}"),
            None => "Play".to_owned(),
        },
        None => "Play".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn numbered(season: i32, episode: i32) -> EpisodeRow {
        EpisodeRow {
            season,
            episode: Some(episode),
            ..EpisodeRow::default()
        }
    }

    #[test]
    fn play_label_follows_selected_season() {
        let episodes = [numbered(2, 1), numbered(1, 2), numbered(1, 1)];
        assert_eq!(play_label(&episodes, 1), "Play S1 E1");
        assert_eq!(play_label(&episodes, 2), "Play S2 E1");
        assert_eq!(play_label(&[], 1), "Play", "a movie");
        assert_eq!(play_label(&[numbered(0, 4)], 0), "Play E4");
    }
}
