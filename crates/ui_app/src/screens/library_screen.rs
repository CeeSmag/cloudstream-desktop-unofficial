use std::collections::BTreeSet;

use core_model::Bookmark;

use crate::actions::UiAction;
use crate::app_model::AppModel;
use crate::navigation::TitleRef;
use crate::theme;
use crate::widgets::buttons;
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
    let filter_id = ui.auto_id_with("lib_filter");
    let selected_filter: String = ui
        .ctx()
        .data_mut(|d| d.get_temp(filter_id))
        .unwrap_or_else(|| "All".to_owned());
    let providers: BTreeSet<&str> = model
        .bookmarks
        .iter()
        .map(|b| b.provider.as_str())
        .collect();

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add_space(theme::PAGE_MARGIN);
                section_title::show(ui, &format!("Saved ({})", model.bookmarks.len()));
            });
            if providers.len() > 1 {
                draw_filter_chips(ui, filter_id, &selected_filter, &providers);
                ui.add_space(10.0);
            }
            let filtered_bookmarks: Vec<_> = model
                .bookmarks
                .iter()
                .filter(|b| selected_filter == "All" || b.provider == selected_filter)
                .collect();
            draw_grid(ui, &filtered_bookmarks, actions);
            ui.add_space(90.0);
        });
}

fn draw_filter_chips(
    ui: &mut egui::Ui,
    filter_id: egui::Id,
    selected: &str,
    providers: &BTreeSet<&str>,
) {
    ui.horizontal(|ui| {
        ui.add_space(theme::PAGE_MARGIN);
        ui.spacing_mut().item_spacing.x = 8.0;
        if buttons::filter_chip(ui, "All", selected == "All").clicked() {
            ui.ctx()
                .data_mut(|d| d.insert_temp(filter_id, "All".to_owned()));
        }
        for provider in providers {
            let is_sel = selected == *provider;
            if buttons::filter_chip(ui, provider, is_sel).clicked() {
                ui.ctx()
                    .data_mut(|d| d.insert_temp(filter_id, (*provider).to_owned()));
            }
        }
    });
}

fn draw_grid(ui: &mut egui::Ui, bookmarks: &[&Bookmark], actions: &mut Vec<UiAction>) {
    ui.horizontal_wrapped(|ui| {
        ui.add_space(theme::PAGE_MARGIN);
        ui.spacing_mut().item_spacing = egui::vec2(18.0, 18.0);
        for bookmark in bookmarks {
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
}
