use core_model::{EpisodeInfo, LoadDetails};
use egui::RichText;

use crate::actions::UiAction;
use crate::app_model::{AppModel, DetailsView, Loadable, PlayTarget};
use crate::theme;
use crate::widgets::{empty_state, section_title};

use app_services::PlaybackItem;

/// A title page: poster, synopsis, play button and episodes.
pub(crate) fn show(
    ui: &mut egui::Ui,
    model: &AppModel,
    view: &DetailsView,
    actions: &mut Vec<UiAction>,
) {
    if ui.button("\u{2190} Back").clicked() {
        actions.push(UiAction::CloseTitle);
    }
    match &view.details {
        Loadable::Idle | Loadable::Loading => empty_state::loading(ui, "Loading\u{2026}"),
        Loadable::Failed(reason) => empty_state::show(ui, "Could not load this title", reason),
        Loadable::Ready(details) => {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let context = DetailsContext {
                        model,
                        view,
                        details,
                    };
                    header(ui, &context, actions);
                    play_controls(ui, &context, actions);
                    episodes(ui, &context, actions);
                });
        }
    }
}

/// The loaded title and the state around it, shared by the sections of the page.
struct DetailsContext<'a> {
    model: &'a AppModel,
    view: &'a DetailsView,
    details: &'a LoadDetails,
}

fn header(ui: &mut egui::Ui, context: &DetailsContext<'_>, actions: &mut Vec<UiAction>) {
    let (view, details) = (context.view, context.details);
    ui.horizontal_top(|ui| {
        if let Some(url) = &details.poster_url {
            ui.add(
                egui::Image::new(url.as_str())
                    .fit_to_exact_size(egui::vec2(
                        theme::POSTER_WIDTH * 1.4,
                        theme::POSTER_HEIGHT * 1.4,
                    ))
                    .corner_radius(theme::ROUNDING),
            );
        }
        ui.vertical(|ui| {
            ui.label(RichText::new(&details.name).size(26.0).strong());
            ui.label(RichText::new(meta_line(details)).color(theme::MUTED_TEXT));
            if let Some(tags) = &details.tags {
                ui.horizontal_wrapped(|ui| {
                    for tag in tags.iter().take(8) {
                        ui.label(
                            RichText::new(tag)
                                .background_color(theme::SURFACE)
                                .color(theme::TEXT),
                        );
                    }
                });
            }
            let star = if view.bookmarked {
                "\u{2605} Saved"
            } else {
                "\u{2606} Save to library"
            };
            if ui.button(star).clicked() {
                actions.push(UiAction::ToggleBookmark);
            }
        });
    });
    if let Some(plot) = &details.plot {
        ui.add_space(8.0);
        ui.label(RichText::new(strip_markup(plot)).color(theme::TEXT));
    }
}

/// Provider and release year, for example `Internet Archive - Movie - 2019`.
fn meta_line(details: &LoadDetails) -> String {
    let mut parts = vec![details.api_name.clone()];
    parts.extend(details.media_type.clone());
    parts.extend(details.year.map(|year| year.to_string()));
    parts.retain(|part| !part.is_empty());
    parts.join(" \u{00B7} ")
}

/// Providers sometimes return HTML line breaks and entities in synopses.
pub(crate) fn strip_markup(text: &str) -> String {
    let mut plain = String::with_capacity(text.len());
    let mut inside_tag = false;
    for character in text.chars() {
        match character {
            '<' => inside_tag = true,
            '>' if inside_tag => {
                inside_tag = false;
                plain.push(' ');
            }
            _ if !inside_tag => plain.push(character),
            _ => {}
        }
    }
    let decoded = plain
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ");
    decoded.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn play_controls(ui: &mut egui::Ui, context: &DetailsContext<'_>, actions: &mut Vec<UiAction>) {
    let (model, view, details) = (context.model, context.view, context.details);
    let Some(data) = details.data_url.as_deref().filter(|data| !data.is_empty()) else {
        return;
    };
    ui.add_space(8.0);
    let label = if model.resolving_streams {
        "Finding streams\u{2026}"
    } else {
        "\u{25B6} Play"
    };
    let button = egui::Button::new(RichText::new(label).strong().size(16.0)).fill(theme::PRIMARY);
    if ui.add_enabled(!model.resolving_streams, button).clicked() {
        actions.push(UiAction::Play(play_target(
            view,
            details,
            data,
            &details.name,
        )));
    }
}

fn play_target(view: &DetailsView, details: &LoadDetails, data: &str, title: &str) -> PlayTarget {
    PlayTarget {
        item: PlaybackItem {
            provider: view.title_ref.provider.clone(),
            media_key: data.to_owned(),
            title: title.to_owned(),
            detail_url: view.title_ref.url.clone(),
            poster_url: details.poster_url.clone(),
        },
        data: data.to_owned(),
    }
}

fn episodes(ui: &mut egui::Ui, context: &DetailsContext<'_>, actions: &mut Vec<UiAction>) {
    let (model, view, details) = (context.model, context.view, context.details);
    if details.episodes.is_empty() {
        return;
    }
    section_title::show(ui, &format!("Episodes ({})", details.episodes.len()));
    for episode in &details.episodes {
        let title = episode_title(episode);
        let row = ui.horizontal(|ui| {
            let play = ui.add_enabled(!model.resolving_streams, egui::Button::new("\u{25B6}"));
            ui.label(RichText::new(&title).color(theme::TEXT));
            play.clicked()
        });
        if row.inner {
            let full_title = format!("{} - {title}", details.name);
            actions.push(UiAction::Play(play_target(
                view,
                details,
                &episode.data,
                &full_title,
            )));
        }
    }
}

/// `S1E3 - Name`, or just the name / number when parts are missing.
pub(crate) fn episode_title(episode: &EpisodeInfo) -> String {
    let number = match (episode.season, episode.episode) {
        (Some(season), Some(number)) => format!("S{season}E{number}"),
        (None, Some(number)) => format!("Episode {number}"),
        _ => String::new(),
    };
    match (
        number.is_empty(),
        episode.name.as_deref().filter(|name| !name.is_empty()),
    ) {
        (false, Some(name)) => format!("{number} - {name}"),
        (false, None) => number,
        (true, Some(name)) => name.to_owned(),
        (true, None) => "Episode".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_markup_removes_tags_and_decodes_basic_entities() {
        assert_eq!(strip_markup("A<br>\n<br>B &amp; C"), "A B & C");
    }

    #[test]
    fn episode_title_combines_number_and_name() {
        let episode = EpisodeInfo {
            season: Some(1),
            episode: Some(3),
            name: Some("Pilot".into()),
            ..EpisodeInfo::default()
        };
        assert_eq!(episode_title(&episode), "S1E3 - Pilot");
    }

    #[test]
    fn episode_title_handles_missing_parts() {
        let only_number = EpisodeInfo {
            episode: Some(7),
            ..EpisodeInfo::default()
        };
        assert_eq!(episode_title(&only_number), "Episode 7");
        assert_eq!(episode_title(&EpisodeInfo::default()), "Episode");
    }

    #[test]
    fn meta_line_skips_missing_values() {
        let details = LoadDetails {
            api_name: "Archive".into(),
            year: Some(2019),
            ..LoadDetails::default()
        };
        assert_eq!(meta_line(&details), "Archive \u{00B7} 2019");
    }
}
