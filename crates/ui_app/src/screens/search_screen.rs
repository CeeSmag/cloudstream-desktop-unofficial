use egui::{FontId, RichText};

use crate::actions::UiAction;
use crate::app_model::AppModel;
use crate::theme;
use crate::widgets::{buttons, empty_state, section_title};

use super::poster_grid;

/// Draws the query box; true when the user submitted it.
fn search_box(ui: &mut egui::Ui, model: &mut AppModel) -> bool {
    ui.horizontal(|ui| {
        ui.add_space(theme::PAGE_MARGIN);
        let field = ui.add(
            egui::TextEdit::singleline(&mut model.search.input)
                .hint_text("Search titles, movies, series\u{2026}")
                .font(FontId::proportional(18.0))
                .margin(egui::Margin::symmetric(16, 10))
                .desired_width(ui.available_width() - 190.0),
        );
        if !model.search.input.is_empty() && buttons::secondary(ui, "\u{2715}").clicked() {
            model.search.input.clear();
        }
        let enter = field.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
        let clicked = buttons::primary(ui, "Search", !model.search.loading).clicked();
        enter || clicked
    })
    .inner
}

/// The search tab: one query box, results grouped by provider.
pub(crate) fn show(ui: &mut egui::Ui, model: &mut AppModel, actions: &mut Vec<UiAction>) {
    ui.add_space(12.0);
    if search_box(ui, model) {
        actions.push(UiAction::Search(model.search.input.clone()));
    }
    ui.add_space(8.0);
    if model.search.loading {
        empty_state::loading(ui, "Searching\u{2026}");
        return;
    }
    if model.search.outcomes.is_empty() {
        let hint = if model.providers.is_empty() {
            "Install an extension in Settings first."
        } else {
            "Type a title and press Enter."
        };
        empty_state::show(ui, "Search", hint);
        return;
    }
    results(ui, model, actions);
}

/// The scrolling list of per-provider results.
fn results(ui: &mut egui::Ui, model: &AppModel, actions: &mut Vec<UiAction>) {
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for outcome in &model.search.outcomes {
                ui.horizontal(|ui| {
                    ui.add_space(theme::PAGE_MARGIN);
                    section_title::show(ui, &outcome.provider);
                });
                match &outcome.result {
                    Err(error) => {
                        ui.horizontal(|ui| {
                            ui.add_space(theme::PAGE_MARGIN);
                            ui.label(
                                RichText::new(format!("Search failed: {error}"))
                                    .color(theme::ERROR),
                            );
                        });
                    }
                    Ok(page) if page.items.is_empty() => {
                        ui.horizontal(|ui| {
                            ui.add_space(theme::PAGE_MARGIN);
                            section_title::muted(ui, "No results");
                        });
                    }
                    Ok(page) => poster_grid::show(ui, &outcome.provider, &page.items, actions),
                }
            }
            ui.add_space(90.0);
        });
}
