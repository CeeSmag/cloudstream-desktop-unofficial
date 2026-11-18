//! Automatic source switch when the playing stream fails.

use crate::app_model::AppModel;
use crate::notice::Notice;

use super::playback_actions::switch_source;
use super::Dispatcher;

/// Tries the next source after a playback failure (check-gate style fallback).
pub(crate) fn try_fallback(dispatcher: &Dispatcher, model: &mut AppModel, reason: &str) -> bool {
    const MAX_ATTEMPTS: u32 = 3;
    if model.fallback_attempts >= MAX_ATTEMPTS {
        return false;
    }
    let next = model.sources.selected.saturating_add(1);
    if next >= model.sources.links.len() {
        return false;
    }
    model.fallback_attempts += 1;
    model.notify(Notice::info(format!(
        "Source failed ({reason}); trying the next stream…"
    )));
    switch_source(dispatcher, model, next);
    true
}

/// Handles end-of-playback: fallback on failure, else close the player screen.
/// Returns true when the video presenter should be reset.
pub(crate) fn close_when_finished(dispatcher: &Dispatcher, model: &mut AppModel) -> bool {
    if !model.player.open {
        return false;
    }
    match &model.playback.state {
        player_api::PlaybackState::Failed(reason) => {
            let reason = reason.clone();
            if try_fallback(dispatcher, model, &reason) {
                return false;
            }
            model.notify(Notice::error(format!("Playback failed: {reason}")));
        }
        player_api::PlaybackState::Ended => {}
        _ => return false,
    }
    if model.playback.state != player_api::PlaybackState::Loading {
        model.player.open = false;
        model.player.fullscreen = false;
        return true;
    }
    false
}
