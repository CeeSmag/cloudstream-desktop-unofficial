//! Settings → Preferences: audio, episodes, subtitles (matches original PreferencesPage).

use app_services::MAX_PREFERRED_SUBTITLES;
use egui::RichText;
use player_api::all_languages;

use crate::actions::UiAction;
use crate::app_model::AppModel;
use crate::theme;
use crate::widgets::section_title;

use super::section_card;

pub(super) fn show(ui: &mut egui::Ui, model: &AppModel, actions: &mut Vec<UiAction>) {
    section_card(ui, |ui| preferred_audio(ui, model, actions));
    section_card(ui, |ui| episodes(ui, model, actions));
    section_card(ui, |ui| preferred_subtitles(ui, model, actions));
}

fn preferred_audio(ui: &mut egui::Ui, model: &AppModel, actions: &mut Vec<UiAction>) {
    section_title::show(ui, "Preferred audio track");
    ui.label(
        RichText::new(
            "When a stream offers several audio tracks, the one in this language plays first. If the stream has no track in this language, its own default track plays. You can still change the track while watching.",
        )
        .size(12.0)
        .color(theme::MUTED_TEXT),
    );
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.label("Language");
        let current = model.settings.preferred_audio.clone();
        let label = current
            .as_deref()
            .and_then(|code| all_languages().iter().find(|l| l.code() == code))
            .map(|l| l.name)
            .unwrap_or("Stream default (no preference)");
        egui::ComboBox::from_id_salt("preferred-audio")
            .selected_text(label)
            .show_ui(ui, |ui| {
                if ui
                    .selectable_label(current.is_none(), "Stream default (no preference)")
                    .clicked()
                {
                    actions.push(UiAction::SetPreferredAudio(None));
                }
                for language in all_languages() {
                    let selected = current.as_deref() == Some(language.code());
                    if ui.selectable_label(selected, language.name).clicked() {
                        actions.push(UiAction::SetPreferredAudio(Some(
                            language.code().to_owned(),
                        )));
                    }
                }
            });
    });
}

fn episodes(ui: &mut egui::Ui, model: &AppModel, actions: &mut Vec<UiAction>) {
    section_title::show(ui, "Episodes");
    option_row(
        ui,
        model.settings.autoplay_next,
        "Play the next episode automatically",
        "When an episode reaches its end, the next one starts by itself.",
        |enabled| actions.push(UiAction::SetAutoplayNext(enabled)),
    );
    option_row(
        ui,
        model.settings.skip_lookup,
        "Look up intro and credits times online",
        "Shows a Skip Intro button when a community database knows where the intro is. This tells that site which title is playing (its IMDb, TMDb or MyAnimeList id), nothing else.",
        |enabled| actions.push(UiAction::SetSkipLookup(enabled)),
    );
    option_row(
        ui,
        model.settings.stream_proxy,
        "Play HLS streams through a local helper",
        "Many sites only serve a stream when every piece of it is asked for with the site's own headers. The helper runs on this PC only and adds them. Switch it off if a stream plays better without it.",
        |enabled| actions.push(UiAction::SetStreamProxy(enabled)),
    );
}

fn preferred_subtitles(ui: &mut egui::Ui, model: &AppModel, actions: &mut Vec<UiAction>) {
    section_title::show(ui, "Preferred subtitles");
    ui.label(
        RichText::new(
            "Choose up to three languages. In the player's Sources menu, each source shows a flag with the first two letters of the language for every one of these languages it has subtitles in.",
        )
        .size(12.0)
        .color(theme::MUTED_TEXT),
    );
    ui.add_space(8.0);
    option_row(
        ui,
        model.settings.auto_select_subtitles,
        "Turn on subtitles automatically",
        "When a video has a subtitle track in one of your preferred languages, it turns on by itself.",
        |enabled| actions.push(UiAction::SetAutoSubtitles(enabled)),
    );
    ui.add_space(6.0);
    let selected = &model.settings.preferred_subtitles;
    for slot in 0..MAX_PREFERRED_SUBTITLES {
        let current = selected.get(slot).cloned();
        let label = current
            .as_deref()
            .and_then(|code| all_languages().iter().find(|l| l.code() == code))
            .map(|l| l.name)
            .unwrap_or("— none —");
        ui.horizontal(|ui| {
            ui.label(format!("Language {}", slot + 1));
            egui::ComboBox::from_id_salt(format!("preferred-sub-{slot}"))
                .selected_text(label)
                .show_ui(ui, |ui| {
                    if ui.selectable_label(current.is_none(), "— none —").clicked() {
                        let mut next = selected.clone();
                        if slot < next.len() {
                            next.remove(slot);
                        }
                        actions.push(UiAction::SetPreferredSubtitles(next));
                    }
                    for language in all_languages() {
                        let code = language.code();
                        let is_selected = current.as_deref() == Some(code);
                        if ui.selectable_label(is_selected, language.name).clicked() {
                            let mut next = selected.clone();
                            if slot < next.len() {
                                if let Some(entry) = next.get_mut(slot) {
                                    *entry = code.to_owned();
                                }
                            } else {
                                next.push(code.to_owned());
                            }
                            // Drop duplicates while keeping order.
                            let mut seen = std::collections::HashSet::new();
                            next.retain(|item| seen.insert(item.clone()));
                            next.truncate(MAX_PREFERRED_SUBTITLES);
                            actions.push(UiAction::SetPreferredSubtitles(next));
                        }
                    }
                });
        });
    }
}

fn option_row(
    ui: &mut egui::Ui,
    checked: bool,
    title: &str,
    hint: &str,
    on_change: impl FnOnce(bool),
) {
    let mut value = checked;
    ui.horizontal(|ui| {
        if ui.checkbox(&mut value, title).changed() {
            on_change(value);
        }
    });
    ui.label(RichText::new(hint).size(11.0).color(theme::MUTED_TEXT));
    ui.add_space(6.0);
}
