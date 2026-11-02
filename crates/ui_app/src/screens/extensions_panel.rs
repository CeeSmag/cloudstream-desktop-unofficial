use core_model::PluginListing;
use egui::RichText;

use crate::actions::UiAction;
use crate::app_model::{AppModel, Loadable};
use crate::theme;
use crate::widgets::{empty_state, section_title};

/// The official CloudStream extensions repository, offered as a one-click start.
const OFFICIAL_REPOSITORY: &str =
    "https://raw.githubusercontent.com/recloudstream/extensions/master/repo.json";

/// Repositories, their plugins and the installed extensions.
pub(crate) fn show(ui: &mut egui::Ui, model: &mut AppModel, actions: &mut Vec<UiAction>) {
    section_title::show(ui, "Extensions");
    add_repository_row(ui, model, actions);
    for repository in model.settings.repositories.clone() {
        egui::CollapsingHeader::new(RichText::new(&repository.name).strong())
            .id_salt(&repository.url)
            .show(ui, |ui| {
                repository_body(ui, model, &repository.url, actions)
            });
    }
    installed_list(ui, model, actions);
}

fn add_repository_row(ui: &mut egui::Ui, model: &mut AppModel, actions: &mut Vec<UiAction>) {
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut model.settings.repository_input)
                .hint_text("Repository URL (repo.json)")
                .desired_width(ui.available_width() - 250.0),
        );
        let busy = model.settings.adding_repository;
        if ui
            .add_enabled(
                !busy && !model.settings.repository_input.trim().is_empty(),
                egui::Button::new("Add"),
            )
            .clicked()
        {
            actions.push(UiAction::AddRepository(
                model.settings.repository_input.clone(),
            ));
        }
        if ui
            .add_enabled(!busy, egui::Button::new("Add official repository"))
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
                plugin_row(ui, &RepositoryContext { model, url }, plugin, actions);
            }
        }
    }
}

/// A repository and the model, shared by its plugin rows.
struct RepositoryContext<'a> {
    model: &'a AppModel,
    url: &'a str,
}

fn plugin_row(
    ui: &mut egui::Ui,
    repository: &RepositoryContext<'_>,
    plugin: &PluginListing,
    actions: &mut Vec<UiAction>,
) {
    let (model, repository_url) = (repository.model, repository.url);
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

fn installed_list(ui: &mut egui::Ui, model: &AppModel, actions: &mut Vec<UiAction>) {
    section_title::show(ui, "Installed");
    if model.settings.installed.is_empty() {
        section_title::muted(
            ui,
            "Nothing installed. Add a repository above, then install a plugin.",
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
                    "Trusted extensions run with full library access. Applies on next start.",
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
