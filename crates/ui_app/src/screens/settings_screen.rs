use egui::RichText;

use crate::actions::UiAction;
use crate::app_model::AppModel;
use crate::theme;
use crate::ui_services::UiServices;
use crate::widgets::{card, section_title};

use super::extensions_panel;

/// Width of the settings column.
const COLUMN_WIDTH: f32 = 840.0;

/// The settings tab: extensions, providers and environment status.
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
                    card::show(ui, |ui| extensions_panel::show(ui, model, actions));
                    ui.add_space(14.0);
                    card::show(ui, |ui| providers_section(ui, model, actions));
                    ui.add_space(14.0);
                    card::show(ui, |ui| status_section(ui, services));
                    ui.add_space(90.0);
                });
            });
        });
}

fn providers_section(ui: &mut egui::Ui, model: &AppModel, actions: &mut Vec<UiAction>) {
    section_title::show(ui, "Providers");
    if model.providers.is_empty() {
        section_title::muted(ui, "No providers loaded yet.");
    }
    for provider in &model.providers {
        let mut enabled = !model.settings.disabled_providers.contains(&provider.name);
        let label = format!("{} ({})", provider.name, provider.lang);
        if ui.checkbox(&mut enabled, label).changed() {
            actions.push(UiAction::SetProviderEnabled {
                provider: provider.name.clone(),
                enabled,
            });
        }
    }
}

fn status_section(ui: &mut egui::Ui, services: &UiServices) {
    section_title::show(ui, "Status");
    match &services.player_problem {
        None => section_title::muted(ui, "Player: mpv ready"),
        Some(problem) => {
            ui.label(RichText::new(format!("Player unavailable: {problem}")).color(theme::ERROR));
        }
    }
}
