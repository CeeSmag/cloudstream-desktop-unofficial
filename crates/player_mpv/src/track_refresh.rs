//! When the event worker should re-read mpv's track and chapter lists.

use player_api::PlaybackState;

/// Periodic track scans are only useful while the stream is active; pause/idle skips the cost.
pub(crate) fn should_refresh_tracks(state: &PlaybackState) -> bool {
    matches!(state, PlaybackState::Playing | PlaybackState::Loading)
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
}
