use core_model::EpisodeInfo;
use egui::{Align2, FontId, Rect, Sense, Stroke, StrokeKind, UiBuilder, Vec2};

use crate::actions::UiAction;
use crate::app_model::PlayTarget;
use crate::screens::details_screen::DetailsContext;
use crate::theme;
use crate::widgets::{buttons, section_title};

/// Renders the titled list of episodes with polished cards and action buttons.
pub(crate) fn show(ui: &mut egui::Ui, context: &DetailsContext<'_>, actions: &mut Vec<UiAction>) {
    let details = context.details;
    if details.episodes.is_empty() {
        return;
    }
    section_title::show(ui, &format!("Episodes ({})", details.episodes.len()));
    for episode in &details.episodes {
        let title = episode_title(episode);
        let code = episode_code(episode);
        if draw_episode_card(ui, &code, &title, context.model.resolving_streams) {
            let full_title = format!("{} - {title}", details.name);
            let target = PlayTarget {
                item: app_services::PlaybackItem {
                    provider: context.view.title_ref.provider.clone(),
                    media_key: episode.data.clone(),
                    title: full_title,
                    detail_url: context.view.title_ref.url.clone(),
                    poster_url: None,
                },
                data: episode.data.clone(),
            };
            actions.push(UiAction::Play(target));
        }
        ui.add_space(6.0);
    }
}

fn draw_episode_card(ui: &mut egui::Ui, code: &str, title: &str, resolving: bool) -> bool {
    let height = 48.0;
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::click());
    let hovered = response.hovered();
    let fill = if hovered {
        theme::SURFACE_HIGH
    } else {
        theme::SURFACE
    };
    let border = if hovered {
        theme::BORDER_HOVER
    } else {
        theme::BORDER_SUBTLE
    };
    ui.painter().rect_filled(rect, theme::ROUNDING, fill);
    ui.painter().rect_stroke(
        rect,
        theme::ROUNDING,
        Stroke::new(1.0, border),
        StrokeKind::Inside,
    );
    draw_code_badge(ui, rect, code);
    draw_title(ui, rect, title);
    let clicked = draw_play_pill(ui, rect, resolving);
    response.clicked() || clicked
}

fn draw_code_badge(ui: &egui::Ui, rect: Rect, code: &str) {
    let badge_rect = Rect::from_center_size(
        egui::pos2(rect.left() + 38.0, rect.center().y),
        Vec2::new(52.0, 26.0),
    );
    ui.painter().rect_filled(badge_rect, 6, theme::ACCENT_SOFT);
    ui.painter().text(
        badge_rect.center(),
        Align2::CENTER_CENTER,
        code,
        FontId::proportional(12.0),
        theme::ACCENT,
    );
}

fn draw_title(ui: &egui::Ui, rect: Rect, title: &str) {
    let title_rect = Rect::from_min_max(
        egui::pos2(rect.left() + 74.0, rect.top()),
        egui::pos2(rect.right() - 90.0, rect.bottom()),
    );
    let galley =
        ui.painter()
            .layout_no_wrap(title.to_owned(), FontId::proportional(14.5), theme::TEXT);
    let y = rect.center().y - galley.size().y / 2.0;
    ui.painter().with_clip_rect(title_rect).galley(
        egui::pos2(title_rect.left(), y),
        galley,
        theme::TEXT,
    );
}

fn draw_play_pill(ui: &mut egui::Ui, rect: Rect, resolving: bool) -> bool {
    let pill_rect = Rect::from_center_size(
        egui::pos2(rect.right() - 48.0, rect.center().y),
        Vec2::new(64.0, 28.0),
    );
    ui.scope_builder(UiBuilder::new().max_rect(pill_rect), |ui| {
        buttons::primary(ui, "Play", !resolving).clicked()
    })
    .inner
}

fn episode_code(episode: &EpisodeInfo) -> String {
    match (episode.season, episode.episode) {
        (Some(season), Some(num)) => format!("S{season} E{num}"),
        (None, Some(num)) => format!("EP {num}"),
        _ => "EP".to_owned(),
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
    fn episode_code_formats_season_and_episode() {
        let ep = EpisodeInfo {
            season: Some(2),
            episode: Some(5),
            ..EpisodeInfo::default()
        };
        assert_eq!(episode_code(&ep), "S2 E5");
    }
}
