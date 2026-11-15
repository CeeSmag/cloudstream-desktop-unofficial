//! Where a show resumes after a save: the episode that played, the next one once this one is
//! (nearly) watched, or nowhere when the last episode or the film is done. Pure, so the rule is
//! tested without a player or a database.

use core_model::ResumePointer;
use player_api::{PlaybackState, PlaybackStatus};

use crate::playback_types::PlaybackItem;

/// Percentage of video watched to advance to the next episode (or mark finished).
pub const NEXT_WATCH_EPISODE_PERCENTAGE: f64 = 0.90;

/// Returns whether playback reached the completion threshold to advance to the next episode.
pub fn is_over_watch_threshold(position_ms: i64, duration_ms: i64) -> bool {
    if duration_ms <= 0 {
        return false;
    }
    (position_ms as f64) / (duration_ms as f64) >= NEXT_WATCH_EPISODE_PERCENTAGE
}

/// What to do with a show's resume pointer after its progress was saved.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ResumeChange {
    /// The item has no title page to resume from (`detail_url` is empty): leave the pointer alone.
    Nothing,
    /// Point the show at this episode.
    Save(ResumePointer),
    /// The last episode or the film is done: the show leaves Continue Watching.
    Remove,
}

/// The pointer for the episode that just played (`item`), or for the next one once it ended or
/// passed `NEXT_WATCH_EPISODE_PERCENTAGE`.
pub(crate) fn resume_change(
    item: &PlaybackItem,
    status: &PlaybackStatus,
    now_ms: i64,
) -> ResumeChange {
    if item.detail_url.is_empty() {
        return ResumeChange::Nothing;
    }
    let finished = status.state == PlaybackState::Ended
        || is_over_watch_threshold(status.position_ms, status.duration_ms);
    let (media_key, season, episode) = match (&item.next_episode, finished) {
        (_, false) => (item.media_key.clone(), item.season, item.episode),
        (Some(next), true) => (next.media_key.clone(), next.season, next.episode),
        (None, true) => return ResumeChange::Remove,
    };
    ResumeChange::Save(ResumePointer {
        provider: item.provider.clone(),
        detail_url: item.detail_url.clone(),
        media_key,
        title: item.title.clone(),
        poster_url: item.poster_url.clone(),
        season,
        episode,
        updated_at_ms: now_ms,
    })
}

#[cfg(test)]
#[path = "playback_resume_tests.rs"]
mod tests;
