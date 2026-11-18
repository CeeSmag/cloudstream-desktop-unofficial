//! Acts on a failed or finished stream: next source, next episode, or close the player.

use app_services::PlaybackItem;
use player_api::PlaybackState;

use crate::playback_watch::{WatchAction, WatchInput};
use crate::CloudStream;

use super::bridge_context::BridgeContext;
use super::details_callbacks::play_episode;
use super::player_callbacks::report;
use super::stream_opening::{begin_open, switch_source};

/// Lets [`crate::playback_watch::PlaybackWatch`] decide on `state`, then does it.
pub(crate) fn react_to_status(ctx: &BridgeContext, ui: &CloudStream, state: &PlaybackState) {
    let action = {
        let mut session = ctx.session.borrow_mut();
        let has_next_episode = session
            .target
            .as_ref()
            .is_some_and(|item| item.next_episode.is_some());
        let sources = session.source_checks.clone();
        let input = WatchInput {
            state,
            selected: session.selected,
            sources: &sources,
            autoplay: ctx.services.settings.autoplay_next().unwrap_or(true),
            has_next_episode,
        };
        session.watch.on_status(&input)
    };
    match action {
        WatchAction::Nothing => {}
        WatchAction::TrySource(index) => {
            tracing::warn!(index, "the playing source failed; trying the next one");
            ui.set_notice("This source stopped working — trying the next one…".into());
            switch_source(ctx, index);
        }
        WatchAction::PlayNextEpisode => play_next_episode(ctx),
        WatchAction::ClosePlayer => close_after_end(ctx, ui, None),
        WatchAction::GiveUp(reason) => close_after_end(ctx, ui, Some(reason)),
    }
}

fn close_after_end(ctx: &BridgeContext, ui: &CloudStream, reason: Option<String>) {
    if let Some(playback) = &ctx.services.playback {
        report("stop", playback.stop());
    }
    ui.set_player_open(false);
    if let Some(reason) = reason {
        tracing::warn!(%reason, "playback gave up");
        ui.set_notice(reason.into());
    }
}

/// Opens the episode after the one that ended: from the title page when it still lists it.
fn play_next_episode(ctx: &BridgeContext) {
    let (target, index) = {
        let session = ctx.session.borrow();
        let Some(target) = session.target.clone() else {
            return;
        };
        let next_key = target
            .next_episode
            .as_ref()
            .map(|next| next.media_key.clone());
        let index =
            next_key.and_then(|key| session.episodes.iter().position(|row| row.data == key));
        (target, index)
    };
    tracing::info!("the episode ended; playing the next one");
    match index {
        Some(index) => play_episode(ctx, index),
        None => {
            if let Some(item) = next_item(&target) {
                let data = item.media_key.clone();
                let shown = item.title.clone();
                begin_open(ctx, item, data, &shown);
            }
        }
    }
}

/// The next episode when the title page is gone (no further episode is known after it).
fn next_item(current: &PlaybackItem) -> Option<PlaybackItem> {
    let next = current.next_episode.clone()?;
    Some(PlaybackItem {
        media_key: next.media_key,
        season: next.season,
        episode: next.episode,
        next_episode: None,
        ..current.clone()
    })
}
