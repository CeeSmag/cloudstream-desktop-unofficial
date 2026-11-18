use egui::RichText;

use crate::actions::UiAction;
use crate::app_model::AppModel;
use crate::theme;
use crate::ui_services::UiServices;
use crate::widgets::{card, section_title};

use super::extensions_panel;

/// Width of the settings column.
const COLUMN_WIDTH: f32 = 840.0;

/// The settings tab: extensions, providers, preferences and environment status.
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
                    card::show(ui, |ui| preferences_section(ui, model, actions));
                    ui.add_space(14.0);
                    card::show(ui, |ui| update_section(ui, model, actions));
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

fn preferences_section(ui: &mut egui::Ui, model: &AppModel, actions: &mut Vec<UiAction>) {
    section_title::show(ui, "Preferences");
    let mut debug = model.settings.debug_logging;
    if ui.checkbox(&mut debug, "Debug logging").changed() {
        actions.push(UiAction::SetDebugLogging(debug));
    }
    let mut proxy = model.settings.stream_proxy;
    if ui
        .checkbox(&mut proxy, "Play HLS streams through a local helper")
        .changed()
    {
        actions.push(UiAction::SetStreamProxy(proxy));
    }
    let mut skip = model.settings.skip_lookup;
    if ui
        .checkbox(&mut skip, "Look up intro / credits skip times online")
        .changed()
    {
        actions.push(UiAction::SetSkipLookup(skip));
    }
    let mut autoplay = model.settings.autoplay_next;
    if ui
        .checkbox(&mut autoplay, "Autoplay next episode")
        .changed()
    {
        actions.push(UiAction::SetAutoplayNext(autoplay));
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

fn update_section(ui: &mut egui::Ui, model: &AppModel, actions: &mut Vec<UiAction>) {
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
        if model.settings.update_url.is_some()
            && ui.button("Open download").clicked()
        {
            actions.push(UiAction::OpenUpdateUrl);
        }
    });
    if let Some(status) = &model.settings.update_status {
        section_title::muted(ui, status);
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
    section_title::muted(ui, "UI: egui (no WebView chrome)");
}
