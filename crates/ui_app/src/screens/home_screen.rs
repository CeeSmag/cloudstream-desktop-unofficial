use egui::RichText;

use crate::actions::UiAction;
use crate::app_model::{AppModel, Loadable};
use crate::navigation::{Tab, TitleRef};
use crate::theme;
use crate::widgets::poster_card::{self, PosterData};
use crate::widgets::{empty_state, section_title};

/// The home tab: provider picker, "continue watching" and the provider's rows.
pub(crate) fn show(ui: &mut egui::Ui, model: &AppModel, actions: &mut Vec<UiAction>) {
    if model.extensions_loading {
        empty_state::loading(ui, "Loading extensions\u{2026}");
        return;
    }
    if model.providers.is_empty() {
        empty_state::show(
            ui,
            "No extensions installed",
            "Open Settings and add a repository to install providers.",
        );
        if ui.button("Go to Settings").clicked() {
            actions.push(UiAction::SwitchTab(Tab::Settings));
        }
        return;
    }
    provider_picker(ui, model, actions);
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            continue_watching_row(ui, model, actions);
            match &model.home {
                Loadable::Idle => {}
                Loadable::Loading => empty_state::loading(ui, "Loading\u{2026}"),
                Loadable::Failed(reason) => failed(ui, reason, actions),
                Loadable::Ready(rows) if rows.is_empty() => empty_state::show(
                    ui,
                    "Nothing here",
                    "This provider has no home page. Use Search to find titles.",
                ),
                Loadable::Ready(rows) => provider_rows(ui, model, rows, actions),
            }
        });
}

fn provider_picker(ui: &mut egui::Ui, model: &AppModel, actions: &mut Vec<UiAction>) {
    ui.horizontal(|ui| {
        let selected = model.selected_provider.clone().unwrap_or_default();
        egui::ComboBox::from_id_salt("provider_picker")
            .selected_text(RichText::new(&selected).strong())
            .show_ui(ui, |ui| {
                for provider in model.enabled_providers() {
                    let is_current = provider.name == selected;
                    if ui.selectable_label(is_current, &provider.name).clicked() && !is_current {
                        actions.push(UiAction::SelectProvider(provider.name.clone()));
                    }
                }
            });
        if ui.button("\u{21BB}").on_hover_text("Reload").clicked() {
            actions.push(UiAction::RefreshHome);
        }
    });
}

fn failed(ui: &mut egui::Ui, reason: &str, actions: &mut Vec<UiAction>) {
    empty_state::show(ui, "Could not load this provider", reason);
    ui.vertical_centered(|ui| {
        if ui.button("Try again").clicked() {
            actions.push(UiAction::RefreshHome);
        }
    });
}

fn continue_watching_row(ui: &mut egui::Ui, model: &AppModel, actions: &mut Vec<UiAction>) {
    if model.continue_watching.is_empty() {
        return;
    }
    section_title::show(ui, "Continue watching");
    egui::ScrollArea::horizontal()
        .id_salt("continue_watching")
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                for entry in &model.continue_watching {
                    let poster = PosterData {
                        title: &entry.title,
                        image_url: entry.poster_url.as_deref(),
                        caption: None,
                        progress: Some(entry.fraction_watched()),
                    };
                    if poster_card::show(ui, &poster) && !entry.detail_url.is_empty() {
                        actions.push(UiAction::OpenTitle(TitleRef {
                            provider: entry.provider.clone(),
                            url: entry.detail_url.clone(),
                        }));
                    }
                }
            });
        });
}

fn provider_rows(
    ui: &mut egui::Ui,
    model: &AppModel,
    rows: &[core_model::HomePageSection],
    actions: &mut Vec<UiAction>,
) {
    let provider = model.selected_provider.clone().unwrap_or_default();
    for (index, row) in rows.iter().enumerate() {
        section_title::show(ui, &row.name);
        egui::ScrollArea::horizontal()
            .id_salt(("home_row", index))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    for item in &row.list {
                        let poster = PosterData {
                            title: &item.name,
                            image_url: item.poster_url.as_deref(),
                            caption: item.media_type.clone(),
                            progress: None,
                        };
                        if poster_card::show(ui, &poster) {
                            actions.push(UiAction::OpenTitle(TitleRef {
                                provider: provider.clone(),
                                url: item.url.clone(),
                            }));
                        }
                    }
                });
            });
    }
    ui.add_space(8.0);
    ui.label(RichText::new("").color(theme::MUTED_TEXT));
}
