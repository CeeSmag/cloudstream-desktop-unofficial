//! Settings → Extensions: Repositories / Installed / Plugins subtabs.

use app_services::PluginHit;
use core_model::PluginListing;
use egui::RichText;

use crate::actions::UiAction;
use crate::app_model::{AppModel, ExtensionsTab, Loadable};
use crate::theme;
use crate::widgets::{empty_state, section_title};

use super::section_card;

/// The official CloudStream extensions repository, offered as a one-click start.
const OFFICIAL_REPOSITORY: &str =
    "https://raw.githubusercontent.com/recloudstream/extensions/master/repo.json";

pub(super) fn show(ui: &mut egui::Ui, model: &mut AppModel, actions: &mut Vec<UiAction>) {
    subtabs(ui, model.settings.extensions_tab, actions);
    ui.add_space(12.0);
    match model.settings.extensions_tab {
        ExtensionsTab::Repositories => section_card(ui, |ui| repositories(ui, model, actions)),
        ExtensionsTab::Installed => section_card(ui, |ui| installed(ui, model, actions)),
        ExtensionsTab::Plugins => section_card(ui, |ui| plugins_catalog(ui, model, actions)),
    }
}

fn subtabs(ui: &mut egui::Ui, current: ExtensionsTab, actions: &mut Vec<UiAction>) {
    ui.horizontal(|ui| {
        for (tab, label) in [
            (ExtensionsTab::Repositories, "Repositories"),
            (ExtensionsTab::Installed, "Installed"),
            (ExtensionsTab::Plugins, "Plugins"),
        ] {
            let selected = current == tab;
            if ui
                .add(egui::Button::new(label).selected(selected))
                .clicked()
            {
                actions.push(UiAction::SetExtensionsTab(tab));
            }
        }
    });
}

fn repositories(ui: &mut egui::Ui, model: &mut AppModel, actions: &mut Vec<UiAction>) {
    section_title::show(ui, "Repositories");
    add_repository_row(ui, model, actions);
    ui.add_space(8.0);
    for repository in model.settings.repositories.clone() {
        egui::CollapsingHeader::new(RichText::new(&repository.name).strong())
            .id_salt(&repository.url)
            .show(ui, |ui| {
                repository_body(ui, model, &repository.url, actions);
            });
    }
}

fn add_repository_row(ui: &mut egui::Ui, model: &mut AppModel, actions: &mut Vec<UiAction>) {
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut model.settings.repository_input)
                .hint_text("Add a repository: https://example.com/repo.json")
                .desired_width(ui.available_width() - 250.0),
        );
        let busy = model.settings.adding_repository;
        if ui
            .add_enabled(
                !busy && !model.settings.repository_input.trim().is_empty(),
                egui::Button::new("+ Add"),
            )
            .clicked()
        {
            actions.push(UiAction::AddRepository(
                model.settings.repository_input.clone(),
            ));
        }
        if ui
            .add_enabled(!busy, egui::Button::new("Add official"))
            .clicked()
        {
            actions.push(UiAction::AddRepository(OFFICIAL_REPOSITORY.to_owned()));
        }
        if busy {
            ui.spinner();
        }
    });
}

fn repository_body(ui: &mut egui::Ui, model: &AppModel, url: &str, actions: &mut Vec<UiAction>) {
    ui.horizontal(|ui| {
        section_title::muted(ui, url);
        if ui.small_button("Remove").clicked() {
            actions.push(UiAction::RemoveRepository(url.to_owned()));
        }
    });
    match model.settings.plugin_lists.get(url) {
        None | Some(Loadable::Idle) => {
            if ui.button("Show available plugins").clicked() {
                actions.push(UiAction::LoadRepositoryPlugins(url.to_owned()));
            }
        }
        Some(Loadable::Loading) => empty_state::loading(ui, "Loading\u{2026}"),
        Some(Loadable::Failed(reason)) => {
            ui.label(RichText::new(reason).color(theme::ERROR));
            if ui.button("Try again").clicked() {
                actions.push(UiAction::LoadRepositoryPlugins(url.to_owned()));
            }
        }
        Some(Loadable::Ready(plugins)) => {
            for plugin in plugins {
                plugin_row(ui, model, url, plugin, actions);
            }
        }
    }
}

fn plugin_row(
    ui: &mut egui::Ui,
    model: &AppModel,
    repository_url: &str,
    plugin: &PluginListing,
    actions: &mut Vec<UiAction>,
) {
    let installed = model
        .settings
        .installed
        .iter()
        .find(|item| item.internal_name == plugin.internal_name);
    let busy = model.settings.busy_plugins.contains(&plugin.internal_name);
    ui.horizontal(|ui| {
        ui.label(RichText::new(&plugin.name).strong());
        section_title::muted(
            ui,
            &format!(
                "v{} {}",
                plugin.version,
                plugin.language.clone().unwrap_or_default()
            ),
        );
        let label = match installed {
            Some(item) if item.version < plugin.version => "Update",
            Some(_) => "Reinstall",
            None => "Install",
        };
        if busy {
            ui.spinner();
        } else if ui.button(label).clicked() {
            actions.push(UiAction::InstallPlugin {
                repository_url: repository_url.to_owned(),
                listing: plugin.clone(),
            });
        }
    });
}

fn installed(ui: &mut egui::Ui, model: &AppModel, actions: &mut Vec<UiAction>) {
    section_title::show(ui, "Installed");
    if model.settings.installed.is_empty() {
        section_title::muted(
            ui,
            "Nothing installed. Add a repository, then install a plugin.",
        );
    }
    for extension in &model.settings.installed {
        ui.horizontal(|ui| {
            ui.label(RichText::new(&extension.internal_name).strong());
            section_title::muted(ui, &format!("v{}", extension.version));
            let mut trusted = extension.trusted;
            if ui
                .checkbox(&mut trusted, "Trusted")
                .on_hover_text(
                    "Trusted extensions skip static bytecode verify and most sandbox denies. Reloads immediately.",
                )
                .changed()
            {
                actions.push(UiAction::SetTrusted {
                    extension: extension.clone(),
                    trusted,
                });
            }
            if ui.small_button("Uninstall").clicked() {
                actions.push(UiAction::UninstallExtension(extension.clone()));
            }
        });
    }
}

fn plugins_catalog(ui: &mut egui::Ui, model: &mut AppModel, actions: &mut Vec<UiAction>) {
    section_title::show(ui, "Plugins");
    ui.horizontal(|ui| {
        if ui.button("Refresh catalog").clicked() {
            actions.push(UiAction::PreparePluginCatalog);
        }
        ui.add(
            egui::TextEdit::singleline(&mut model.settings.plugin_catalog_filter)
                .hint_text("Search plugins")
                .desired_width(240.0),
        );
    });
    ui.add_space(8.0);
    match &model.settings.plugin_catalog {
        Loadable::Idle => {
            section_title::muted(ui, "Press Refresh catalog to list plugins from every repository.");
            if ui.button("Load catalog").clicked() {
                actions.push(UiAction::PreparePluginCatalog);
            }
        }
        Loadable::Loading => empty_state::loading(ui, "Building catalog\u{2026}"),
        Loadable::Failed(reason) => {
            ui.label(RichText::new(reason).color(theme::ERROR));
            if ui.button("Try again").clicked() {
                actions.push(UiAction::PreparePluginCatalog);
            }
        }
        Loadable::Ready(hits) => {
            let filter = model.settings.plugin_catalog_filter.to_ascii_lowercase();
            let filtered: Vec<&PluginHit> = hits
                .iter()
                .filter(|hit| {
                    filter.is_empty()
                        || hit.plugin.name.to_ascii_lowercase().contains(&filter)
                        || hit
                            .plugin
                            .internal_name
                            .to_ascii_lowercase()
                            .contains(&filter)
                        || hit.repository_name.to_ascii_lowercase().contains(&filter)
                })
                .collect();
            section_title::muted(ui, &format!("{} plugins", filtered.len()));
            for hit in filtered {
                catalog_row(ui, model, hit, actions);
            }
        }
    }
}

fn catalog_row(ui: &mut egui::Ui, model: &AppModel, hit: &PluginHit, actions: &mut Vec<UiAction>) {
    let plugin = &hit.plugin;
    let installed = model
        .settings
        .installed
        .iter()
        .find(|item| item.internal_name == plugin.internal_name);
    let busy = model.settings.busy_plugins.contains(&plugin.internal_name);
    ui.horizontal(|ui| {
        ui.label(RichText::new(&plugin.name).strong());
        section_title::muted(
            ui,
            &format!("v{} · {}", plugin.version, hit.repository_name),
        );
        let label = match installed {
            Some(item) if item.version < plugin.version => "Update",
            Some(_) => "Reinstall",
            None => "Install",
        };
        if busy {
            ui.spinner();
        } else if ui.button(label).clicked() {
            actions.push(UiAction::InstallPlugin {
                repository_url: hit.repository_url.clone(),
                listing: plugin.clone(),
            });
        }
    });
}
