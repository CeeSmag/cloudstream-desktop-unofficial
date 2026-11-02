use crate::actions::UiAction;
use crate::app_model::AppModel;
use crate::navigation::TitleRef;
use crate::theme;
use crate::widgets::poster_card::{self, PosterData};
use crate::widgets::{empty_state, section_title};

/// The library tab: saved titles.
pub(crate) fn show(ui: &mut egui::Ui, model: &AppModel, actions: &mut Vec<UiAction>) {
    if model.bookmarks.is_empty() {
        empty_state::show(
            ui,
            "Your library is empty",
            "Open a title and choose Save to keep it here.",
        );
        return;
    }
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add_space(theme::PAGE_MARGIN);
                section_title::show(ui, "Saved");
            });
            ui.horizontal_wrapped(|ui| {
                ui.add_space(theme::PAGE_MARGIN);
                ui.spacing_mut().item_spacing = egui::vec2(18.0, 18.0);
                for bookmark in &model.bookmarks {
                    let poster = PosterData {
                        title: &bookmark.name,
                        image_url: bookmark.poster_url.as_deref(),
                        caption: Some(bookmark.provider.clone()),
                        progress: None,
                    };
                    if poster_card::show(ui, &poster) {
                        actions.push(UiAction::OpenTitle(TitleRef {
                            provider: bookmark.provider.clone(),
                            url: bookmark.url.clone(),
                        }));
                    }
                }
            });
            ui.add_space(90.0);
        });
}
