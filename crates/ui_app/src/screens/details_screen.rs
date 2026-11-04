use app_services::PlaybackItem;
use core_model::LoadDetails;
use egui::RichText;

use crate::actions::UiAction;
use crate::app_model::{AppModel, DetailsView, Loadable, PlayTarget};
use crate::theme;
use crate::widgets::hero_banner::{self, HeroButtons, HeroClick, HeroData};
use crate::widgets::{buttons, empty_state};

use super::details_episodes;
pub(crate) use super::details_episodes::episode_title;

/// Height of the backdrop banner on a title page.
const BACKDROP_HEIGHT: f32 = 330.0;
/// Widest the synopsis may grow, for comfortable line lengths.
const READING_WIDTH: f32 = 780.0;

/// The loaded title and the state around it, shared by the sections of the page.
pub(crate) struct DetailsContext<'a> {
    pub(crate) model: &'a AppModel,
    pub(crate) view: &'a DetailsView,
    pub(crate) details: &'a LoadDetails,
}

/// A title page: backdrop with title and actions, synopsis, genres and episodes.
pub(crate) fn show(
    ui: &mut egui::Ui,
    model: &AppModel,
    view: &DetailsView,
    actions: &mut Vec<UiAction>,
) {
    match &view.details {
        Loadable::Idle | Loadable::Loading => {
            back_button(ui, actions);
            empty_state::loading(ui, "Loading\u{2026}");
        }
        Loadable::Failed(reason) => {
            back_button(ui, actions);
            empty_state::show(ui, "Could not load this title", reason);
        }
        Loadable::Ready(details) => {
            let context = DetailsContext {
                model,
                view,
                details,
            };
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    backdrop(ui, &context, actions);
                    body(ui, &context, actions);
                    ui.add_space(90.0);
                });
        }
    }
}

fn back_button(ui: &mut egui::Ui, actions: &mut Vec<UiAction>) {
    ui.horizontal(|ui| {
        ui.add_space(theme::PAGE_MARGIN);
        if buttons::secondary(ui, "\u{2190} Back").clicked() {
            actions.push(UiAction::CloseTitle);
        }
    });
}

fn backdrop(ui: &mut egui::Ui, context: &DetailsContext<'_>, actions: &mut Vec<UiAction>) {
    let (view, details) = (context.view, context.details);
    let playable = playable_data(details).is_some();
    let label = if context.model.resolving_streams {
        "Finding streams\u{2026}"
    } else {
        "Play"
    };
    let save = if view.bookmarked { "Saved" } else { "+ Save" };
    let hero = HeroData {
        title: &details.name,
        eyebrow: meta_line(details),
        synopsis: None,
        image_url: details.poster_url.as_deref(),
    };
    let buttons = HeroButtons {
        primary: label,
        primary_enabled: playable && !context.model.resolving_streams,
        secondary: Some(save),
    };
    let top = ui.cursor().top();
    let clicked = hero_banner::show(ui, &hero, BACKDROP_HEIGHT, &buttons);
    match clicked {
        Some(HeroClick::Primary) => {
            if let Some((data, title)) = playable_data(details) {
                actions.push(UiAction::Play(play_target(view, details, &data, &title)));
            }
        }
        Some(HeroClick::Secondary) => actions.push(UiAction::ToggleBookmark),
        None => {}
    }
    let back = egui::Rect::from_min_size(
        egui::pos2(ui.max_rect().left() + theme::PAGE_MARGIN, top + 14.0),
        egui::vec2(96.0, 36.0),
    );
    if floating_back(ui, back) {
        actions.push(UiAction::CloseTitle);
    }
}

/// A glass "Back" pill painted over the backdrop without taking layout space.
fn floating_back(ui: &mut egui::Ui, rect: egui::Rect) -> bool {
    let response = ui.interact(rect, ui.auto_id_with("floating_back"), egui::Sense::click());
    let fill = if response.hovered() {
        egui::Color32::from_white_alpha(70)
    } else {
        egui::Color32::from_black_alpha(120)
    };
    ui.painter().rect_filled(rect, theme::PILL_ROUNDING, fill);
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        "\u{2190} Back",
        egui::FontId::proportional(15.0),
        theme::TEXT,
    );
    response.clicked()
}

/// The payload to play for this title: the movie data, else the first episode.
fn playable_data(details: &LoadDetails) -> Option<(String, String)> {
    if let Some(data) = details.data_url.as_deref().filter(|data| !data.is_empty()) {
        return Some((data.to_owned(), details.name.clone()));
    }
    let first = details.episodes.first()?;
    Some((
        first.data.clone(),
        format!("{} - {}", details.name, episode_title(first)),
    ))
}

fn body(ui: &mut egui::Ui, context: &DetailsContext<'_>, actions: &mut Vec<UiAction>) {
    let details = context.details;
    let reading_width =
        (ui.available_width() - theme::PAGE_MARGIN * 2.0).clamp(240.0, READING_WIDTH);
    ui.horizontal(|ui| {
        ui.add_space(theme::PAGE_MARGIN);
        ui.vertical(|ui| {
            ui.set_width(reading_width);
            if let Some(tags) = details.tags.as_ref().filter(|tags| !tags.is_empty()) {
                tags_row(ui, tags, reading_width);
            }
            if let Some(plot) = &details.plot {
                ui.add_space(10.0);
                ui.label(
                    RichText::new(strip_markup(plot))
                        .size(16.0)
                        .color(theme::TEXT),
                );
            }
            details_episodes::show(ui, context, actions);
        });
    });
}

/// Horizontal padding inside a tag chip, and the gap between chips.
const CHIP_PADDING: f32 = 12.0;
const CHIP_GAP: f32 = 6.0;

/// Lays tags out in rows no wider than `max_width` (measured, so labels never get squeezed).
fn tags_row(ui: &mut egui::Ui, tags: &[String], max_width: f32) {
    let mut rows: Vec<Vec<&str>> = vec![Vec::new()];
    let mut used = 0.0;
    for tag in tags.iter().take(10) {
        let width = chip_width(ui, tag);
        if used + width > max_width && used > 0.0 {
            rows.push(Vec::new());
            used = 0.0;
        }
        if let Some(row) = rows.last_mut() {
            row.push(tag);
        }
        used += width + CHIP_GAP;
    }
    for row in rows {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = CHIP_GAP;
            for tag in row {
                chip(ui, tag);
            }
        });
    }
}

fn chip_width(ui: &egui::Ui, tag: &str) -> f32 {
    let font = egui::FontId::proportional(13.0);
    ui.painter()
        .layout_no_wrap(tag.to_owned(), font, theme::TEXT)
        .size()
        .x
        + CHIP_PADDING * 2.0
}

fn chip(ui: &mut egui::Ui, tag: &str) {
    egui::Frame::new()
        .fill(theme::SURFACE_HIGH)
        .corner_radius(theme::PILL_ROUNDING)
        .inner_margin(egui::Margin::symmetric(12, 4))
        .show(ui, |ui| {
            ui.add(egui::Label::new(RichText::new(tag).size(13.0).color(theme::TEXT)).extend());
        });
}

/// Provider, content kind and release year, for example `Internet Archive \u{00B7} Movie \u{00B7} 2019`.
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

fn play_target(view: &DetailsView, details: &LoadDetails, data: &str, title: &str) -> PlayTarget {
    PlayTarget {
        item: PlaybackItem {
            provider: view.title_ref.provider.clone(),
            media_key: data.to_owned(),
            title: title.to_owned(),
            detail_url: view.title_ref.url.clone(),
            poster_url: details.poster_url.clone(),
            season: None,
            episode: None,
            next_episode: None,
        },
        data: data.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core_model::EpisodeInfo;

    #[test]
    fn strip_markup_removes_tags_and_decodes_basic_entities() {
        assert_eq!(strip_markup("A<br>\n<br>B &amp; C"), "A B & C");
    }

    #[test]
    fn meta_line_skips_missing_values() {
        assert_eq!(
            meta_line(&LoadDetails {
                api_name: "Archive".into(),
                year: Some(2019),
                ..LoadDetails::default()
            }),
            "Archive \u{00B7} 2019"
        );
    }

    #[test]
    fn playable_data_prefers_movie_data_then_first_episode() {
        let movie = LoadDetails {
            name: "M".into(),
            data_url: Some("id".into()),
            ..LoadDetails::default()
        };
        assert_eq!(
            playable_data(&movie),
            Some(("id".to_owned(), "M".to_owned()))
        );
        let series = LoadDetails {
            name: "S".into(),
            episodes: vec![EpisodeInfo {
                data: "ep".into(),
                episode: Some(1),
                ..EpisodeInfo::default()
            }],
            ..LoadDetails::default()
        };
        assert_eq!(
            playable_data(&series).map(|found| found.0),
            Some("ep".to_owned())
        );
        assert_eq!(playable_data(&LoadDetails::default()), None);
    }
}
