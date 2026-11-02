use std::collections::{HashMap, HashSet};

use app_services::{PlaybackItem, ProviderSearchOutcome};
use core_model::{
    Bookmark, HomePageSection, InstalledExtension, LoadDetails, PlayableLink, PluginListing,
    ProviderInfo, RepositoryEntry, SubtitleTrack, WatchProgress,
};
use player_api::{MediaTrack, PlaybackStatus};

use crate::navigation::{Tab, TitleRef};
use crate::notice::Notice;

/// Data that is being fetched in the background.
#[derive(Debug, Clone, Default)]
pub(crate) enum Loadable<T> {
    /// Nothing requested yet.
    #[default]
    Idle,
    /// Request in flight.
    Loading,
    /// Data arrived.
    Ready(T),
    /// Request failed; carries a readable reason.
    Failed(String),
}

/// What to play and how to remember it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PlayTarget {
    pub(crate) item: PlaybackItem,
    /// Provider payload passed to link loading.
    pub(crate) data: String,
}

/// Several streams were found; the user picks one.
#[derive(Debug, Clone)]
pub(crate) struct PendingStreams {
    pub(crate) target: PlayTarget,
    pub(crate) links: Vec<PlayableLink>,
    pub(crate) subtitles: Vec<SubtitleTrack>,
}

/// One open title page.
#[derive(Debug, Clone)]
pub(crate) struct DetailsView {
    pub(crate) title_ref: TitleRef,
    pub(crate) details: Loadable<LoadDetails>,
    pub(crate) bookmarked: bool,
}

/// State of the search tab.
#[derive(Default)]
pub(crate) struct SearchState {
    pub(crate) input: String,
    pub(crate) loading: bool,
    pub(crate) last_query: String,
    pub(crate) outcomes: Vec<ProviderSearchOutcome>,
}

/// State of the settings tab.
#[derive(Default)]
pub(crate) struct SettingsState {
    pub(crate) repository_input: String,
    pub(crate) repositories: Vec<RepositoryEntry>,
    pub(crate) plugin_lists: HashMap<String, Loadable<Vec<PluginListing>>>,
    pub(crate) installed: Vec<InstalledExtension>,
    pub(crate) disabled_providers: Vec<String>,
    pub(crate) busy_plugins: HashSet<String>,
    pub(crate) adding_repository: bool,
}

/// The pop-up menus of the video screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PlayerMenu {
    /// Audio track choice.
    Audio,
    /// Subtitle choice.
    Subtitles,
    /// Source (stream) choice.
    Sources,
}

/// The streams found for what is playing, so the user can switch between them.
#[derive(Debug, Clone, Default)]
pub(crate) struct SourceSet {
    pub(crate) target: Option<PlayTarget>,
    pub(crate) links: Vec<PlayableLink>,
    pub(crate) subtitles: Vec<SubtitleTrack>,
    pub(crate) selected: usize,
}

/// State of the embedded video screen.
#[derive(Debug, Clone, Default)]
pub(crate) struct PlayerView {
    /// The video screen replaces the normal screens while this is true.
    pub(crate) open: bool,
    /// The window is fullscreen.
    pub(crate) fullscreen: bool,
    /// Set when the screen just opened, so the controls start visible.
    pub(crate) just_opened: bool,
    /// Egui time of the last pointer or key activity.
    pub(crate) last_activity: f64,
    /// The open pop-up menu, if any.
    pub(crate) menu: Option<PlayerMenu>,
}

/// Everything the screens draw.
#[derive(Default)]
pub(crate) struct AppModel {
    pub(crate) tab: Tab,
    pub(crate) providers: Vec<ProviderInfo>,
    pub(crate) extensions_loading: bool,
    pub(crate) selected_provider: Option<String>,
    pub(crate) home: Loadable<Vec<HomePageSection>>,
    pub(crate) continue_watching: Vec<WatchProgress>,
    pub(crate) search: SearchState,
    pub(crate) details_stack: Vec<DetailsView>,
    pub(crate) bookmarks: Vec<Bookmark>,
    pub(crate) resolving_streams: bool,
    pub(crate) pending_streams: Option<PendingStreams>,
    pub(crate) now_playing_title: Option<String>,
    pub(crate) player: PlayerView,
    pub(crate) tracks: Vec<MediaTrack>,
    pub(crate) sources: SourceSet,
    pub(crate) playback: PlaybackStatus,
    pub(crate) settings: SettingsState,
    pub(crate) notices: Vec<Notice>,
}

impl AppModel {
    /// Providers the user has not switched off.
    pub(crate) fn enabled_providers(&self) -> Vec<ProviderInfo> {
        self.providers
            .iter()
            .filter(|provider| !self.settings.disabled_providers.contains(&provider.name))
            .cloned()
            .collect()
    }

    /// The provider whose home page is shown, if it is still available.
    pub(crate) fn selected_provider_info(&self) -> Option<&ProviderInfo> {
        let name = self.selected_provider.as_deref()?;
        self.providers.iter().find(|provider| provider.name == name)
    }

    /// Adds a message to the notice bar, dropping exact duplicates.
    pub(crate) fn notify(&mut self, notice: Notice) {
        if !self.notices.contains(&notice) {
            self.notices.push(notice);
        }
    }

    /// The title page currently on top, if any.
    pub(crate) fn current_details(&self) -> Option<&DetailsView> {
        self.details_stack.last()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider(name: &str) -> ProviderInfo {
        ProviderInfo {
            name: name.to_owned(),
            ..ProviderInfo::default()
        }
    }

    #[test]
    fn enabled_providers_excludes_disabled_ones() {
        let mut model = AppModel {
            providers: vec![provider("A"), provider("B")],
            ..AppModel::default()
        };
        model.settings.disabled_providers = vec!["A".to_owned()];
        let names: Vec<_> = model
            .enabled_providers()
            .into_iter()
            .map(|p| p.name)
            .collect();
        assert_eq!(names, ["B"]);
    }

    #[test]
    fn notify_ignores_exact_duplicates() {
        let mut model = AppModel::default();
        model.notify(Notice::error("boom"));
        model.notify(Notice::error("boom"));
        model.notify(Notice::info("boom"));
        assert_eq!(model.notices.len(), 2);
    }

    #[test]
    fn selected_provider_info_is_none_when_provider_disappeared() {
        let model = AppModel {
            selected_provider: Some("Gone".to_owned()),
            providers: vec![provider("Other")],
            ..AppModel::default()
        };
        assert!(model.selected_provider_info().is_none());
    }
}
