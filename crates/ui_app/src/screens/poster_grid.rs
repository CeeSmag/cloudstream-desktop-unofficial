use core_model::SearchResult;

use crate::actions::UiAction;
use crate::navigation::TitleRef;
use crate::widgets::poster_card::{self, PosterData};

/// Draws search results as a wrapping grid and emits `OpenTitle` for a click.
pub(crate) fn show(
    ui: &mut egui::Ui,
    provider: &str,
    items: &[SearchResult],
    actions: &mut Vec<UiAction>,
) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(12.0, 12.0);
        for item in items {
            let poster = PosterData {
                title: &item.name,
                image_url: item.poster_url.as_deref(),
                caption: item.media_type.clone(),
                progress: None,
            };
            if poster_card::show(ui, &poster) {
                actions.push(UiAction::OpenTitle(TitleRef {
                    provider: provider.to_owned(),
                    url: item.url.clone(),
                }));
            }
        }
    });
}
