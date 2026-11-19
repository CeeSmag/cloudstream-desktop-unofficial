//! What to do when the playing stream fails or ends.
//!
//! The Slint shell had neither: a failed source just stopped (the egui shell tried the next one),
//! and auto-advance was an empty stub. This is the rule, kept free of the player and the window so
//! every case can be tested: on failure try the next source that is not known to be broken, at most
//! [`MAX_FALLBACKS`] times per opened video; when an episode ends, play the next one once (when
//! "Autoplay next episode" is on), else close the player.

use player_api::PlaybackState;

/// Sources tried after a failure before giving up, per opened video.
pub(crate) const MAX_FALLBACKS: u32 = 3;

/// How each source of the open list fared in its check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SourceCheck {
    Working,
    Broken,
    Unchecked,
}

/// What the screen knows when a status arrives.
#[derive(Debug, Clone)]
pub(crate) struct WatchInput<'a> {
    pub(crate) state: &'a PlaybackState,
    /// Index of the playing source.
    pub(crate) selected: usize,
    pub(crate) sources: &'a [SourceCheck],
    pub(crate) autoplay: bool,
    pub(crate) has_next_episode: bool,
}

/// The step to take.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum WatchAction {
    Nothing,
    /// Play source `index` from the current position.
    TrySource(usize),
    PlayNextEpisode,
    /// Close the player (the video ended and nothing follows).
    ClosePlayer,
    /// Every attempt failed: close the player and say why.
    GiveUp(String),
}

/// Remembers what was already done for the opened video.
#[derive(Debug, Default)]
pub(crate) struct PlaybackWatch {
    fallbacks: u32,
    end_handled: bool,
    failure_handled: bool,
}

impl PlaybackWatch {
    /// A new video was opened (or another episode started).
    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }

    /// The step for the latest status.
    pub(crate) fn on_status(&mut self, input: &WatchInput<'_>) -> WatchAction {
        match input.state {
            PlaybackState::Failed(reason) => self.on_failure(input, reason),
            PlaybackState::Ended => self.on_end(input),
            // Loading clears the latch too: after TrySource the player often goes
            // Loading → Failed without Playing; without this, further fallbacks stall.
            PlaybackState::Playing | PlaybackState::Loading => {
                self.failure_handled = false;
                WatchAction::Nothing
            }
            _ => WatchAction::Nothing,
        }
    }

    fn on_failure(&mut self, input: &WatchInput<'_>, reason: &str) -> WatchAction {
        if self.failure_handled {
            return WatchAction::Nothing;
        }
        self.failure_handled = true;
        if self.fallbacks >= MAX_FALLBACKS {
            return WatchAction::GiveUp(format!("Playback failed: {reason}"));
        }
        match next_usable_source(input.sources, input.selected) {
            Some(index) => {
                self.fallbacks += 1;
                WatchAction::TrySource(index)
            }
            None => WatchAction::GiveUp(format!(
                "Playback failed and no other source works: {reason}"
            )),
        }
    }

    fn on_end(&mut self, input: &WatchInput<'_>) -> WatchAction {
        if self.end_handled {
            return WatchAction::Nothing;
        }
        self.end_handled = true;
        if input.autoplay && input.has_next_episode {
            WatchAction::PlayNextEpisode
        } else {
            WatchAction::ClosePlayer
        }
    }
}

/// Marks the playing source broken so wrap-around fallback will not retry it.
pub(crate) fn mark_selected_broken(sources: &mut [SourceCheck], selected: usize) {
    if let Some(slot) = sources.get_mut(selected) {
        *slot = SourceCheck::Broken;
    }
}

/// The first source after `selected` (wrapping) that is not known to be broken.
pub(crate) fn next_usable_source(sources: &[SourceCheck], selected: usize) -> Option<usize> {
    let count = sources.len();
    (1..count)
        .map(|step| (selected + step) % count)
        .find(|index| {
            sources
                .get(*index)
                .is_some_and(|check| *check != SourceCheck::Broken)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    use SourceCheck::{Broken, Unchecked, Working};

    fn input<'a>(state: &'a PlaybackState, sources: &'a [SourceCheck]) -> WatchInput<'a> {
        WatchInput {
            state,
            selected: 0,
            sources,
            autoplay: true,
            has_next_episode: true,
        }
    }

    #[test]
    fn on_status_failure_skips_broken_sources() {
        let mut watch = PlaybackWatch::default();
        let failed = PlaybackState::Failed("403".into());
        let sources = [Working, Broken, Unchecked, Working];
        assert_eq!(
            watch.on_status(&input(&failed, &sources)),
            WatchAction::TrySource(2)
        );
    }

    #[test]
    fn on_status_same_failure_reported_twice_acts_once() {
        let mut watch = PlaybackWatch::default();
        let failed = PlaybackState::Failed("403".into());
        let sources = [Working, Working];
        assert_eq!(
            watch.on_status(&input(&failed, &sources)),
            WatchAction::TrySource(1)
        );
        assert_eq!(
            watch.on_status(&input(&failed, &sources)),
            WatchAction::Nothing
        );
    }

    #[test]
    fn on_status_loading_after_try_source_allows_another_fallback() {
        // TrySource puts mpv in Loading; if that source fails without ever Playing,
        // the next Failed must still fall over (not stay stuck on failure_handled).
        let mut watch = PlaybackWatch::default();
        let failed = PlaybackState::Failed("403".into());
        let loading = PlaybackState::Loading;
        let sources = [Working, Working, Working];
        assert_eq!(
            watch.on_status(&input(&failed, &sources)),
            WatchAction::TrySource(1)
        );
        assert_eq!(
            watch.on_status(&input(&loading, &sources)),
            WatchAction::Nothing
        );
        let second = WatchInput {
            selected: 1,
            ..input(&failed, &sources)
        };
        assert_eq!(watch.on_status(&second), WatchAction::TrySource(2));
    }

    #[test]
    fn on_status_gives_up_after_max_fallbacks() {
        let mut watch = PlaybackWatch::default();
        let failed = PlaybackState::Failed("dead".into());
        let playing = PlaybackState::Playing;
        let sources = [Working; 6];
        for _ in 0..MAX_FALLBACKS {
            assert!(matches!(
                watch.on_status(&input(&failed, &sources)),
                WatchAction::TrySource(_)
            ));
            watch.on_status(&input(&playing, &sources));
        }
        assert!(matches!(
            watch.on_status(&input(&failed, &sources)),
            WatchAction::GiveUp(_)
        ));
    }

    #[test]
    fn on_status_no_other_source_gives_up() {
        let mut watch = PlaybackWatch::default();
        let failed = PlaybackState::Failed("dead".into());
        assert!(matches!(
            watch.on_status(&input(&failed, &[Working])),
            WatchAction::GiveUp(_)
        ));
        let mut watch = PlaybackWatch::default();
        assert!(matches!(
            watch.on_status(&input(&failed, &[Working, Broken])),
            WatchAction::GiveUp(_)
        ));
    }

    #[test]
    fn mark_selected_broken_keeps_wrap_around_from_retrying_the_failed_url() {
        let mut sources = [Working, Working];
        let mut watch = PlaybackWatch::default();
        let failed = PlaybackState::Failed("dead".into());
        assert_eq!(
            watch.on_status(&input(&failed, &sources)),
            WatchAction::TrySource(1)
        );
        mark_selected_broken(&mut sources, 0);
        watch.on_status(&input(&PlaybackState::Loading, &sources));
        let second = WatchInput {
            selected: 1,
            ..input(&failed, &sources)
        };
        // Without marking, next_usable_source would wrap back to 0.
        assert!(
            matches!(watch.on_status(&second), WatchAction::GiveUp(_)),
            "the only other source already failed"
        );
        assert_eq!(next_usable_source(&sources, 1), None);
    }

    #[test]
    fn on_status_end_plays_next_once_or_closes() {
        let ended = PlaybackState::Ended;
        let mut watch = PlaybackWatch::default();
        assert_eq!(
            watch.on_status(&input(&ended, &[Working])),
            WatchAction::PlayNextEpisode
        );
        assert_eq!(
            watch.on_status(&input(&ended, &[Working])),
            WatchAction::Nothing
        );
        let mut watch = PlaybackWatch::default();
        let no_autoplay = WatchInput {
            autoplay: false,
            ..input(&ended, &[Working])
        };
        assert_eq!(watch.on_status(&no_autoplay), WatchAction::ClosePlayer);
        watch.reset();
        let last_episode = WatchInput {
            has_next_episode: false,
            ..input(&ended, &[Working])
        };
        assert_eq!(watch.on_status(&last_episode), WatchAction::ClosePlayer);
    }
}
