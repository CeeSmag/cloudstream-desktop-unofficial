//! Settings pages mirroring the original General / Preferences / Extensions shell.

mod extensions;
mod general;
mod preferences;

use egui::RichText;

use crate::actions::UiAction;
use crate::app_model::{AppModel, SettingsPage};
use crate::theme;
use crate::ui_services::UiServices;
use crate::widgets::card;

/// Width of the settings column.
const COLUMN_WIDTH: f32 = 840.0;

/// The settings tab: page switcher plus the active page body.
pub(crate) fn show(
    ui: &mut egui::Ui,
    model: &mut AppModel,
    services: &UiServices,
    actions: &mut Vec<UiAction>,
) {
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add_space(theme::PAGE_MARGIN);
                ui.vertical(|ui| {
                    ui.set_max_width(COLUMN_WIDTH);
                    ui.add_space(12.0);
                    header(ui, model, actions);
                    ui.add_space(16.0);
                    match model.settings.page {
                        SettingsPage::General => general::show(ui, model, services, actions),
                        SettingsPage::Preferences => preferences::show(ui, model, actions),
                        SettingsPage::Extensions => extensions::show(ui, model, actions),
                    }
                    ui.add_space(90.0);
                });
            });
        });
}

fn header(ui: &mut egui::Ui, model: &AppModel, actions: &mut Vec<UiAction>) {
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(
                RichText::new("Settings")
                    .strong()
                    .size(28.0)
                    .color(theme::TEXT),
            );
            ui.label(
                RichText::new("Extensions, repositories and how the app runs")
                    .size(12.0)
                    .color(theme::MUTED_TEXT),
            );
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            page_switcher(ui, model.settings.page, actions);
        });
    });
}

fn page_switcher(ui: &mut egui::Ui, current: SettingsPage, actions: &mut Vec<UiAction>) {
    egui::Frame::new()
        .fill(theme::SURFACE)
        .corner_radius(10)
        .inner_margin(egui::Margin::same(4))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                for (page, label) in [
                    (SettingsPage::General, "General"),
                    (SettingsPage::Preferences, "Preferences"),
                    (SettingsPage::Extensions, "Extensions"),
                ] {
                    let selected = current == page;
                    let button = egui::Button::new(RichText::new(label).size(12.0)).selected(selected);
                    if ui.add(button).clicked() {
                        actions.push(UiAction::SetSettingsPage(page));
                    }
                }
            });
        });
}

/// Shared card wrapper used by the page modules.
fn section_card(ui: &mut egui::Ui, body: impl FnOnce(&mut egui::Ui)) {
    card::show(ui, body);
    ui.add_space(14.0);
}
