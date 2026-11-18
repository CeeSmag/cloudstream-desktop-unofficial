//! In-app GitHub update check for the egui product.

use crate::app_model::AppModel;
use crate::notice::Notice;
use crate::ui_event::UiEvent;

use super::{Dispatcher, UiAction};

/// Handles update-related actions; returns true when the action was consumed.
pub(super) fn perform(dispatcher: &Dispatcher, model: &mut AppModel, action: &UiAction) -> bool {
    match action {
        UiAction::CheckForUpdate => {
            model.settings.checking_update = true;
            model.settings.update_status.replace("Checking…".to_owned());
            let current = env!("CARGO_PKG_VERSION").to_owned();
            dispatcher.runner.spawn("check-update", move || {
                UiEvent::UpdateChecked(crate::update_check::check_for_update(&current))
            });
            true
        }
        UiAction::OpenUpdateUrl => {
            if let Some(url) = &model.settings.update_url {
                if let Err(error) = open::that(url) {
                    model.notify(Notice::error(format!("Cannot open update URL: {error}")));
                }
            }
            true
        }
        _ => false,
    }
}
