//! The title page: play, resume, episodes, seasons, close.

use app_services::{NextEpisode, PlaybackItem};
use core_model::Bookmark;

use crate::posters::empty_episodes;
use crate::request_tickets::RequestKind;
use crate::ui_events::EpisodeRow;
use crate::CloudStream;

use super::bridge_context::BridgeContext;
use super::catalog_events::show_library;
use super::details_loading::parse_season_label;
use super::details_view::show_episode_list;
use super::focus_navigation::restore_focus;
use super::home_callbacks::row;
use super::session::DetailsTarget;
use super::stream_opening::begin_open;

pub(crate) fn wire_details(ctx: &BridgeContext, ui: &CloudStream) {
    let c = ctx.clone();
    ui.on_play_details(move || play_details(&c));
    let c = ctx.clone();
    ui.on_resume_details(move || resume_details(&c));
    let c = ctx.clone();
    ui.on_play_episode(move |index| play_episode(&c, row(index)));
    let c = ctx.clone();
    ui.on_close_details(move || close_details(&c));
    let c = ctx.clone();
    ui.on_toggle_bookmark(move || toggle_bookmark(&c));
    let c = ctx.clone();
    ui.on_skip_checks(move || c.session.borrow().control.skip_checks());
    let c = ctx.clone();
    ui.on_season_chosen(move |label| {
        if let Some(ui) = c.window() {
            ui.set_season_filter(parse_season_label(&label));
            show_episode_list(&c, &ui);
        }
    });
}

/// Closes the title page; its late answer and a video still opening from it are dropped.
fn close_details(ctx: &BridgeContext) {
    let Some(ui) = ctx.window() else {
        return;
    };
    {
        let mut session = ctx.session.borrow_mut();
        session.tickets.cancel(RequestKind::Details);
        if ui.get_open_busy() {
            session.control.cancel_open();
            session.tickets.cancel(RequestKind::Open);
        }
    }
    ui.set_open_busy(false);
    ui.set_open_status("".into());
    ui.set_details_title("".into());
    ui.set_episodes(empty_episodes());
    restore_focus(ctx, &ui);
}

/// Adds the title page's title to the Library, or takes it out.
fn toggle_bookmark(ctx: &BridgeContext) {
    let Some(ui) = ctx.window() else {
        return;
    };
    let details = ctx.session.borrow().details.clone();
    let library = &ctx.services.library;
    let result = if ui.get_details_bookmarked() {
        library.remove_bookmark(&details.provider, &details.url)
    } else {
        library.add_bookmark(&Bookmark {
            provider: details.provider.clone(),
            url: details.url.clone(),
            name: ui.get_details_title().to_string(),
            poster_url: details.poster_url.clone(),
            media_type: None,
            added_at_ms: ctx.services.clock.now_ms(),
        })
    };
    match result {
        Ok(()) => ui.set_details_bookmarked(!ui.get_details_bookmarked()),
        Err(error) => {
            tracing::warn!(%error, "the bookmark could not be changed");
            ui.set_notice("Could not change the Library".into());
        }
    }
    show_library(ctx, &ui);
}

/// Plays the movie (or the title's own stream when it lists no episodes).
fn play_details(ctx: &BridgeContext) {
    let title = ctx
        .window()
        .map(|ui| ui.get_details_title().to_string())
        .unwrap_or_default();
    let details = ctx.session.borrow().details.clone();
    let item = PlaybackItem {
        provider: details.provider.clone(),
        media_key: details.media_key.clone(),
        title: title.clone(),
        detail_url: details.url.clone(),
        poster_url: details.poster_url.clone(),
        season: None,
        episode: None,
        next_episode: None,
    };
    begin_open(ctx, item, details.data, &title);
}

/// Plays where the viewer left off: the remembered episode, else the title itself.
fn resume_details(ctx: &BridgeContext) {
    let resume_index = {
        let session = ctx.session.borrow();
        let wanted = session.details.resume_media_key.clone();
        wanted.and_then(|key| session.episodes.iter().position(|row| row.data == key))
    };
    match resume_index {
        Some(index) => play_episode(ctx, index),
        None => play_details(ctx),
    }
}

/// Plays episode `index` of the title page (provider order).
pub(crate) fn play_episode(ctx: &BridgeContext, index: usize) {
    let show_title = ctx
        .window()
        .map(|ui| ui.get_details_title().to_string())
        .unwrap_or_default();
    let (details, episodes) = {
        let session = ctx.session.borrow();
        (session.details.clone(), session.episodes.clone())
    };
    let Some(item) = episode_item(&details, &episodes, index, &show_title) else {
        return;
    };
    let shown = episodes
        .get(index)
        .map(|episode| format!("{show_title} — {} {}", episode.code, episode.name))
        .unwrap_or_else(|| show_title.clone());
    let data = item.media_key.clone();
    begin_open(ctx, item, data, shown.trim());
}

/// The item for episode `index`: saved under the show's title with its season and episode, and the
/// following episode for auto-advance.
pub(crate) fn episode_item(
    details: &DetailsTarget,
    episodes: &[EpisodeRow],
    index: usize,
    show_title: &str,
) -> Option<PlaybackItem> {
    let episode = episodes.get(index)?;
    let next_episode = episodes.get(index + 1).map(|next| NextEpisode {
        media_key: next.data.clone(),
        season: (next.season >= 0).then_some(next.season),
        episode: next.episode,
    });
    Some(PlaybackItem {
        provider: details.provider.clone(),
        media_key: episode.data.clone(),
        title: show_title.to_owned(),
        detail_url: details.url.clone(),
        poster_url: details.poster_url.clone(),
        season: (episode.season >= 0).then_some(episode.season),
        episode: episode.episode,
        next_episode,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(data: &str, season: i32, episode: Option<i32>) -> EpisodeRow {
        EpisodeRow {
            name: format!("Episode {data}"),
            code: String::new(),
            data: data.to_owned(),
            watched: false,
            season,
            episode,
        }
    }

    #[test]
    fn episode_item_carries_numbers_poster_and_next_episode() {
        let details = DetailsTarget {
            provider: "P".into(),
            url: "https://p/show".into(),
            poster_url: Some("https://p/poster.jpg".into()),
            ..DetailsTarget::default()
        };
        let episodes = [row("e1", 1, Some(1)), row("e2", 1, Some(2))];
        let first = episode_item(&details, &episodes, 0, "Show").expect("first");
        assert_eq!((first.season, first.episode), (Some(1), Some(1)));
        assert_eq!(first.title, "Show");
        assert_eq!(first.poster_url.as_deref(), Some("https://p/poster.jpg"));
        assert_eq!(
            first.next_episode.map(|next| next.media_key),
            Some("e2".to_owned())
        );
        let last = episode_item(&details, &episodes, 1, "Show").expect("last");
        assert!(last.next_episode.is_none());
        assert!(episode_item(&details, &episodes, 2, "Show").is_none());
    }

    #[test]
    fn episode_item_unknown_season_is_none() {
        let details = DetailsTarget::default();
        let item = episode_item(&details, &[row("e", -1, None)], 0, "Show").expect("item");
        assert_eq!((item.season, item.episode), (None, None));
    }
}
