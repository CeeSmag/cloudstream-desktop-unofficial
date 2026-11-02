use core_model::{HomePageSection, SearchResult};
use egui::RichText;

use crate::actions::UiAction;
use crate::app_model::{AppModel, Loadable};
use crate::navigation::{Tab, TitleRef};
use crate::theme;
use crate::widgets::hero_banner::{self, HeroButtons, HeroClick, HeroData};
use crate::widgets::poster_card::PosterData;
use crate::widgets::{buttons, empty_state, shelf};

/// Height of the hero banner.
const HERO_HEIGHT: f32 = 340.0;

/// The home tab: hero banner, "continue watching" and the selected provider's shelves.
pub(crate) fn show(ui: &mut egui::Ui, model: &AppModel, actions: &mut Vec<UiAction>) {
    if model.extensions_loading {
        empty_state::loading(ui, "Loading extensions\u{2026}");
        return;
    }
    if model.providers.is_empty() {
        onboarding(ui, actions);
        return;
    }
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            match &model.home {
                Loadable::Ready(rows) if !rows.is_empty() => {
                    hero(ui, model, rows, actions);
                    provider_picker(ui, model, actions);
                }
                _ => provider_picker(ui, model, actions),
            }
            continue_watching(ui, model, actions);
            body(ui, model, actions);
            ui.add_space(90.0);
        });
}

fn onboarding(ui: &mut egui::Ui, actions: &mut Vec<UiAction>) {
    empty_state::show(
        ui,
        "No extensions installed",
        "Open Settings and add a repository to install providers.",
    );
    ui.vertical_centered(|ui| {
        if buttons::primary(ui, "Open Settings", true).clicked() {
            actions.push(UiAction::SwitchTab(Tab::Settings));
        }
    });
}

fn body(ui: &mut egui::Ui, model: &AppModel, actions: &mut Vec<UiAction>) {
    match &model.home {
        Loadable::Idle => {}
        Loadable::Loading => empty_state::loading(ui, "Loading\u{2026}"),
        Loadable::Failed(reason) => failed(ui, reason, actions),
        Loadable::Ready(rows) if rows.is_empty() => {
            empty_state::show(
                ui,
                "Nothing here",
                "This provider has no home page. Use Search to find titles.",
            );
        }
        Loadable::Ready(rows) => shelves(ui, model, rows, actions),
    }
}

fn hero(
    ui: &mut egui::Ui,
    model: &AppModel,
    rows: &[HomePageSection],
    actions: &mut Vec<UiAction>,
) {
    let Some(item) = rows.iter().find_map(|row| row.list.first()) else {
        return;
    };
    let provider = model.selected_provider.clone().unwrap_or_default();
    let data = HeroData {
        title: &item.name,
        eyebrow: eyebrow(&provider, item),
        synopsis: None,
        image_url: item.poster_url.as_deref(),
    };
    let buttons = HeroButtons {
        primary: "Open",
        primary_enabled: true,
        secondary: None,
    };
    if hero_banner::show(ui, &data, HERO_HEIGHT, &buttons) == Some(HeroClick::Primary) {
        actions.push(UiAction::OpenTitle(TitleRef {
            provider,
            url: item.url.clone(),
        }));
    }
}

fn eyebrow(provider: &str, item: &SearchResult) -> String {
    match &item.media_type {
        Some(kind) => format!("{provider} \u{00B7} {kind}"),
        None => provider.to_owned(),
    }
}

fn provider_picker(ui: &mut egui::Ui, model: &AppModel, actions: &mut Vec<UiAction>) {
    ui.horizontal(|ui| {
        ui.add_space(theme::PAGE_MARGIN);
        ui.label(RichText::new("Source").color(theme::MUTED_TEXT));
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
        if buttons::secondary(ui, "Reload").clicked() {
            actions.push(UiAction::RefreshHome);
        }
    });
}

fn failed(ui: &mut egui::Ui, reason: &str, actions: &mut Vec<UiAction>) {
    empty_state::show(ui, "Could not load this provider", reason);
    ui.vertical_centered(|ui| {
        if buttons::primary(ui, "Try again", true).clicked() {
            actions.push(UiAction::RefreshHome);
        }
    });
}

fn continue_watching(ui: &mut egui::Ui, model: &AppModel, actions: &mut Vec<UiAction>) {
    if model.continue_watching.is_empty() {
        return;
    }
    let posters: Vec<PosterData<'_>> = model
        .continue_watching
        .iter()
        .map(|entry| PosterData {
            title: &entry.title,
            image_url: entry.poster_url.as_deref(),
            caption: None,
            progress: Some(entry.fraction_watched()),
        })
        .collect();
    if let Some(index) = shelf::show(ui, "continue_watching", "Continue Watching", &posters) {
        if let Some(entry) = model
            .continue_watching
            .get(index)
            .filter(|entry| !entry.detail_url.is_empty())
        {
            actions.push(UiAction::OpenTitle(TitleRef {
                provider: entry.provider.clone(),
                url: entry.detail_url.clone(),
            }));
        }
    }
}

fn shelves(
    ui: &mut egui::Ui,
    model: &AppModel,
    rows: &[HomePageSection],
    actions: &mut Vec<UiAction>,
) {
    let provider = model.selected_provider.clone().unwrap_or_default();
    for (index, row) in rows.iter().enumerate() {
        let posters: Vec<PosterData<'_>> = row
            .list
            .iter()
            .map(|item| PosterData {
                title: &item.name,
                image_url: item.poster_url.as_deref(),
                caption: item.media_type.clone(),
                progress: None,
            })
            .collect();
        if let Some(clicked) = shelf::show(ui, ("home_row", index), &row.name, &posters) {
            if let Some(item) = row.list.get(clicked) {
                actions.push(UiAction::OpenTitle(TitleRef {
                    provider: provider.clone(),
                    url: item.url.clone(),
                }));
            }
        }
    }
}
