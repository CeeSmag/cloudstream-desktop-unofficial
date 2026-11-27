//! Mutually exclusive loading priority focus between Sources and Subtitles.

/// User-selected resource focus while sources and/or subtitles are still loading.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum LoadFocus {
    /// Automatic policy: full parallel checks before playback; after Play-now, pause stream checks
    /// for up to 8 s until preferred subtitles are ready.
    #[default]
    Auto,
    /// Prioritize finding and checking video sources; temporarily pause subtitle downloads.
    Sources,
    /// Prioritize finding and downloading subtitles; pause stream checks until search finishes.
    Subtitles,
}

impl LoadFocus {
    /// Toggles Sources focus on or off (turning off returns to [`Self::Auto`]).
    pub(crate) fn toggle_sources(self) -> Self {
        match self {
            Self::Sources => Self::Auto,
            Self::Auto | Self::Subtitles => Self::Sources,
        }
    }

    /// Toggles Subtitles focus on or off (turning off returns to [`Self::Auto`]).
    pub(crate) fn toggle_subtitles(self) -> Self {
        match self {
            Self::Subtitles => Self::Auto,
            Self::Auto | Self::Sources => Self::Subtitles,
        }
    }

    /// Releases manual Subtitles pin when the active stream fails and needs a fallback source.
    pub(crate) fn on_playback_failed(self) -> Self {
        match self {
            Self::Subtitles => Self::Auto,
            Self::Auto | Self::Sources => self,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::LoadFocus;

    #[test]
    fn toggling_sources_and_subtitles_is_mutually_exclusive() {
        let state = LoadFocus::default();
        assert_eq!(state, LoadFocus::Auto);
        let state = state.toggle_sources();
        assert_eq!(state, LoadFocus::Sources);
        let state = state.toggle_subtitles();
        assert_eq!(state, LoadFocus::Subtitles);
        let state = state.toggle_subtitles();
        assert_eq!(state, LoadFocus::Auto);
        let state = state.toggle_sources();
        assert_eq!(state, LoadFocus::Sources);
        let state = state.toggle_sources();
        assert_eq!(state, LoadFocus::Auto);
    }

    #[test]
    fn playback_failure_releases_subtitles_pin_but_preserves_sources_focus() {
        assert_eq!(LoadFocus::Subtitles.on_playback_failed(), LoadFocus::Auto);
        assert_eq!(LoadFocus::Sources.on_playback_failed(), LoadFocus::Sources);
        assert_eq!(LoadFocus::Auto.on_playback_failed(), LoadFocus::Auto);
    }
}
