//! Builds a title page from the provider's answer and the viewer's history.

use app_services::UiServices;
use core_model::{CastMember, LoadDetails, WatchProgress};
use slint::{ModelRc, VecModel};

use crate::text_sanitize::clean_display_text;
use crate::ui_events::{DetailsPage, EpisodeRow};
use crate::{CastItem, DetailsMeta};

use super::session::DetailsTarget;

/// Maximum cast portraits displayed in the horizontal Cast strip.
const MAX_CAST: usize = 20;

/// The page for `details`, with where the viewer left off and which episodes they watched.
pub(crate) fn details_page(
    services: &UiServices,
    target: DetailsTarget,
    details: LoadDetails,
    fallback_title: String,
) -> DetailsPage {
    let pointer = load_resume_pointer(services, &target);
    let progress = services
        .library
        .show_progress(&target.provider, &target.url)
        .unwrap_or_default();
    let data = match &details.data_url {
        Some(data_url) if details.episodes.is_empty() => data_url.clone(),
        _ => target.url.clone(),
    };
    let (title, plot, tags) = clean_details_text(&details, &fallback_title);
    let episodes = episode_rows(&details, &progress);
    DetailsPage {
        title,
        plot,
        resume: pointer.as_ref().map(resume_label).unwrap_or_default(),
        resume_media_key: pointer.map(|pointer| pointer.media_key),
        episodes,
        poster_url: details.poster_url,
        background_url: details.background_poster_url,
        provider: target.provider,
        url: target.url,
        data,
        media_key: target.media_key,
        skip_ids: skip_times::ExternalIds::from_sync(
            &details.sync_data,
            details.unique_url.as_deref(),
        ),
        media_type: details.media_type.unwrap_or_default(),
        year: details.year,
        rating: details.rating,
        duration_minutes: details.duration,
        tags,
        cast_members: details.actors,
    }
}

fn clean_details_text(
    details: &LoadDetails,
    fallback_title: &str,
) -> (String, String, Vec<String>) {
    let raw_title = if details.name.is_empty() {
        fallback_title
    } else {
        details.name.as_str()
    };
    let title = clean_display_text(raw_title);
    let plot = details
        .plot
        .as_deref()
        .map(clean_display_text)
        .unwrap_or_default();
    let tags = details
        .tags
        .as_deref()
        .unwrap_or(&[])
        .iter()
        .map(|tag| clean_display_text(tag))
        .collect();
    (title, plot, tags)
}

fn load_resume_pointer(services: &UiServices, target: &DetailsTarget) -> Option<WatchProgress> {
    match services
        .library
        .resume_pointer(&target.provider, &target.url)
    {
        Ok(pointer) => pointer,
        Err(error) => {
            tracing::warn!(%error, "could not read where the title was left off");
            None
        }
    }
}

/// Structured header badges and facts shown above the title.
pub(crate) fn format_details_meta(page: &DetailsPage) -> DetailsMeta {
    DetailsMeta {
        media_type: page.media_type.trim().to_uppercase().into(),
        provider: page.provider.clone().into(),
        year: page
            .year
            .filter(|year| *year > 0)
            .map(|year| year.to_string())
            .unwrap_or_default()
            .into(),
        rating: page
            .rating
            .filter(|rating| *rating > 0)
            .map(|rating| format!("{:.1}", f64::from(rating) / 1000.0))
            .unwrap_or_default()
            .into(),
        duration: page
            .duration_minutes
            .filter(|minutes| *minutes > 0)
            .map(|minutes| format!("{minutes} min"))
            .unwrap_or_default()
            .into(),
    }
}

/// Converts provider cast members into Slint `CastItem` rows (capped at [`MAX_CAST`]).
pub(crate) fn cast_items_model(members: &[CastMember]) -> ModelRc<CastItem> {
    let rows: Vec<CastItem> = members
        .iter()
        .filter(|member| !member.name.trim().is_empty())
        .take(MAX_CAST)
        .map(|member| {
            let name = clean_display_text(member.name.trim());
            let role = member
                .role
                .as_deref()
                .map(|text| clean_display_text(text.trim()))
                .unwrap_or_default();
            let initial = name
                .chars()
                .next()
                .map(|ch| ch.to_uppercase().to_string())
                .unwrap_or_default();
            CastItem {
                name: name.into(),
                role: role.into(),
                initial: initial.into(),
                photo_url: member.image.clone().unwrap_or_default().into(),
                image: slint::Image::default(),
            }
        })
        .collect();
    ModelRc::new(VecModel::from(rows))
}

/// From this share of an episode on it counts as watched.
pub(crate) const WATCHED_FRACTION: f32 = 0.8;

/// How far a saved position went (0 to 1) and whether that counts as watched; never started (or
/// an unknown length) is 0 and not watched.
pub(crate) fn episode_mark(position_ms: i64, duration_ms: i64) -> (f32, bool) {
    if duration_ms <= 0 || position_ms <= 0 {
        return (0.0, false);
    }
    let fraction = (position_ms as f64 / duration_ms as f64).min(1.0) as f32;
    (fraction, fraction >= WATCHED_FRACTION)
}

/// Remaining playback time such as `"1h 2m left"` or `"23m left"`, or `None` when finished/unknown.
pub(crate) fn format_remaining_time(position_ms: i64, duration_ms: i64) -> Option<String> {
    if duration_ms <= 0 || position_ms >= duration_ms {
        return None;
    }
    let remaining_ms = (duration_ms - position_ms).max(0);
    let total_minutes = ((remaining_ms as f64 / 60_000.0).round() as i64).max(1);
    let hours = total_minutes / 60;
    let minutes = total_minutes % 60;
    Some(match (hours, minutes) {
        (h, m) if h > 0 && m > 0 => format!("{h}h {m}m left"),
        (h, _) if h > 0 => format!("{h}h left"),
        (_, m) => format!("{m}m left"),
    })
}

/// `"Resume S3 E1 (1h 2m left)"`, `"Resume (45m left)"`, or `"Resume"`.
pub(crate) fn resume_label(pointer: &WatchProgress) -> String {
    let place = match (pointer.season, pointer.episode) {
        (Some(season), Some(episode)) => format!("Resume S{season} E{episode}"),
        (None, Some(episode)) => format!("Resume E{episode}"),
        _ => "Resume".to_owned(),
    };
    match format_remaining_time(pointer.position_ms, pointer.duration_ms) {
        Some(time_left) => format!("{place} ({time_left})"),
        None => place,
    }
}

fn episode_rows(details: &LoadDetails, progress: &[WatchProgress]) -> Vec<EpisodeRow> {
    details
        .episodes
        .iter()
        .map(|episode| episode_row(episode, progress))
        .collect()
}

fn episode_row(episode: &core_model::EpisodeInfo, progress: &[WatchProgress]) -> EpisodeRow {
    let code = match (episode.season, episode.episode) {
        (Some(season), Some(number)) => format!("S{season}E{number}"),
        (None, Some(number)) => format!("E{number}"),
        _ => String::new(),
    };
    let episode_label = match episode.episode {
        Some(number) => format!("Episode {number}"),
        None => "Episode".to_owned(),
    };
    let (fraction, watched) = progress
        .iter()
        .find(|seen| seen.media_key == episode.data)
        .map_or((0.0, false), |seen| {
            episode_mark(seen.position_ms, seen.duration_ms)
        });
    let name = episode
        .name
        .as_deref()
        .map(clean_display_text)
        .unwrap_or_else(|| code.clone());
    let description = episode
        .description
        .as_deref()
        .map(clean_display_text)
        .unwrap_or_default();
    EpisodeRow {
        name,
        code,
        episode_label,
        watched,
        progress: fraction,
        description,
        data: episode.data.clone(),
        season: episode.season.unwrap_or(-1),
        episode: episode.episode,
        poster_url: episode.poster_url.clone(),
    }
}

#[cfg(test)]
#[path = "details_page_tests.rs"]
mod tests;
