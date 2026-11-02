use crate::app_model::AppModel;
use crate::task_runner::TaskRunner;
use crate::ui_event::UiEvent;
use crate::ui_services::UiServices;

use super::{browse_actions, playback_actions, settings_actions, UiAction};

/// Performs `UiAction`s by calling services, spawning slow work on the task runner.
pub(crate) struct Dispatcher {
    pub(super) services: UiServices,
    pub(super) runner: TaskRunner,
}

impl Dispatcher {
    pub(crate) fn new(services: UiServices, runner: TaskRunner) -> Self {
        Self { services, runner }
    }

    /// Events produced by finished background work.
    pub(crate) fn drain_events(&self) -> Vec<UiEvent> {
        self.runner.drain()
    }

    /// Providers the extension service currently knows.
    pub(crate) fn providers(&self) -> Vec<core_model::ProviderInfo> {
        self.services.extensions.providers()
    }

    /// Reloads everything that lives in the local database.
    pub(crate) fn refresh_local_state(&self, model: &mut AppModel) {
        settings_actions::refresh_local_state(self, model);
    }

    /// Loads installed extensions and local data once at launch.
    pub(crate) fn start_up(&self, model: &mut AppModel) {
        settings_actions::refresh_local_state(self, model);
        settings_actions::load_installed_extensions(self, model);
    }

    /// Carries out one action.
    pub(crate) fn perform(&self, model: &mut AppModel, action: UiAction) {
        match action {
            UiAction::SwitchTab(_)
            | UiAction::SelectProvider(_)
            | UiAction::RefreshHome
            | UiAction::Search(_)
            | UiAction::OpenTitle(_)
            | UiAction::CloseTitle
            | UiAction::ToggleBookmark => browse_actions::perform(self, model, action),
            UiAction::Play(_)
            | UiAction::PlayLink(_)
            | UiAction::DismissStreamChoice
            | UiAction::SetPaused(_)
            | UiAction::SeekBy(_)
            | UiAction::SeekTo(_)
            | UiAction::TogglePause
            | UiAction::OpenPlayer
            | UiAction::ClosePlayer
            | UiAction::ToggleFullscreen
            | UiAction::StopPlayback => playback_actions::perform(self, model, action),
            UiAction::DismissNotice(index) => {
                if index < model.notices.len() {
                    model.notices.remove(index);
                }
            }
            other => settings_actions::perform(self, model, other),
        }
    }
}
