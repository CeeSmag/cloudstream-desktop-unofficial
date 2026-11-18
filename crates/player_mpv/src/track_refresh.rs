//! When the event worker should re-read mpv's track and chapter lists.

use player_api::PlaybackState;

/// The track list is re-read every this many event iterations (about once a second).
pub(crate) const TRACK_REFRESH_EVERY: u32 = 10;

/// Periodic track scans are only useful while the stream is active; pause/idle skips the cost.
pub(crate) fn should_refresh_tracks(state: &PlaybackState) -> bool {
    matches!(state, PlaybackState::Playing | PlaybackState::Loading)
}

/// True when this tick should re-read tracks and chapters.
pub(crate) fn due(ticks: u32, state: &PlaybackState) -> bool {
    ticks.is_multiple_of(TRACK_REFRESH_EVERY) && should_refresh_tracks(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paused_ended_and_idle_skip_track_refresh() {
        assert!(should_refresh_tracks(&PlaybackState::Playing));
        assert!(should_refresh_tracks(&PlaybackState::Loading));
        assert!(!should_refresh_tracks(&PlaybackState::Paused));
        assert!(!should_refresh_tracks(&PlaybackState::Idle));
        assert!(!should_refresh_tracks(&PlaybackState::Ended));
    }

    #[test]
    fn due_only_on_refresh_ticks_while_playing() {
        assert!(due(TRACK_REFRESH_EVERY, &PlaybackState::Playing));
        assert!(!due(1, &PlaybackState::Playing));
        assert!(!due(TRACK_REFRESH_EVERY, &PlaybackState::Paused));
    }
}
