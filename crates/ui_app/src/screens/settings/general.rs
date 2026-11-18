//! Settings → General: updates, debug, workers, providers, status.

use egui::RichText;

use crate::actions::UiAction;
use crate::app_model::AppModel;
use crate::theme;
use crate::ui_services::UiServices;
use crate::widgets::section_title;

use super::section_card;

pub(super) fn show(
    ui: &mut egui::Ui,
    model: &mut AppModel,
    services: &UiServices,
    actions: &mut Vec<UiAction>,
) {
    section_card(ui, |ui| updates(ui, model, actions));
    section_card(ui, |ui| debug_and_workers(ui, model, actions));
    section_card(ui, |ui| providers(ui, model, actions));
    section_card(ui, |ui| status(ui, services));
}

fn updates(ui: &mut egui::Ui, model: &AppModel, actions: &mut Vec<UiAction>) {
    section_title::show(ui, "Updates");
    ui.label(format!("Running version {}", env!("CARGO_PKG_VERSION")));
    ui.horizontal(|ui| {
        let checking = model.settings.checking_update;
        if ui
            .add_enabled(!checking, egui::Button::new("Check for updates"))
            .clicked()
        {
            actions.push(UiAction::CheckForUpdate);
        }
        if model.settings.update_url.is_some() && ui.button("Open download").clicked() {
            actions.push(UiAction::OpenUpdateUrl);
        }
    });
    if let Some(status) = &model.settings.update_status {
        section_title::muted(ui, status);
    }
}

fn debug_and_workers(ui: &mut egui::Ui, model: &AppModel, actions: &mut Vec<UiAction>) {
    section_title::show(ui, "Runtime");
    let mut debug = model.settings.debug_logging;
    if ui.checkbox(&mut debug, "Debug logging").changed() {
        actions.push(UiAction::SetDebugLogging(debug));
    }
    ui.add_space(8.0);
    ui.label("Plugin processes kept ready (low-end PCs: use 1)");
    ui.horizontal(|ui| {
        for (label, value) in [("1", 1u32), ("2", 2), ("Unlimited", 0)] {
            let selected = model.settings.plugin_worker_limit == value;
            if ui.selectable_label(selected, label).clicked() {
                actions.push(UiAction::SetPluginWorkerLimit(value));
            }
        }
    });
}

fn providers(ui: &mut egui::Ui, model: &AppModel, actions: &mut Vec<UiAction>) {
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

fn status(ui: &mut egui::Ui, services: &UiServices) {
    section_title::show(ui, "Status");
    match &services.player_problem {
        None => section_title::muted(ui, "Player: mpv ready"),
        Some(problem) => {
            ui.label(RichText::new(format!("Player unavailable: {problem}")).color(theme::ERROR));
        }
    }
    section_title::muted(ui, "UI: egui (no WebView chrome)");
}
