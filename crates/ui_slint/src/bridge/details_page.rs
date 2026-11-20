//! Builds a title page from the provider's answer and the viewer's history.

use app_services::UiServices;
use core_model::{CastMember, LoadDetails, WatchProgress};
use slint::{ModelRc, VecModel};

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
    let title = if details.name.is_empty() {
        fallback_title
    } else {
        details.name.clone()
    };
    DetailsPage {
        title,
        plot: details.plot.clone().unwrap_or_default(),
        resume: pointer.as_ref().map(resume_label).unwrap_or_default(),
        resume_media_key: pointer.map(|pointer| pointer.media_key),
        episodes: episode_rows(&details, &progress),
        poster_url: details.poster_url.clone(),
        background_url: details.background_poster_url.clone(),
        provider: target.provider,
        url: target.url,
        data,
        media_key: target.media_key,
        skip_ids: skip_times::ExternalIds::from_sync(
            &details.sync_data,
            details.unique_url.as_deref(),
        ),
        media_type: details.media_type.clone().unwrap_or_default(),
        year: details.year,
        rating: details.rating,
        duration_minutes: details.duration,
        tags: details.tags.clone().unwrap_or_default(),
        cast_members: details.actors,
    }
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
            let trimmed = member.name.trim();
            let initial = trimmed
                .chars()
                .next()
                .map(|ch| ch.to_uppercase().to_string())
                .unwrap_or_default();
            CastItem {
                name: trimmed.into(),
                role: member.role.as_deref().map(str::trim).unwrap_or("").into(),
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
        .map(|episode| {
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
            EpisodeRow {
                name: episode.name.clone().unwrap_or_else(|| code.clone()),
                code,
                episode_label,
                watched,
                progress: fraction,
                description: episode.description.clone().unwrap_or_default(),
                data: episode.data.clone(),
                season: episode.season.unwrap_or(-1),
                episode: episode.episode,
                poster_url: episode.poster_url.clone(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resume_label_formats_hours_minutes_and_movies() {
        let hours = WatchProgress {
            season: Some(3),
            episode: Some(1),
            position_ms: 8 * 60_000,
            duration_ms: 70 * 60_000,
            ..WatchProgress::default()
        };
        assert_eq!(resume_label(&hours), "Resume S3 E1 (1h 2m left)");
        let mins = WatchProgress {
            season: Some(1),
            episode: Some(3),
            position_ms: 10 * 60_000,
            duration_ms: 33 * 60_000,
            ..WatchProgress::default()
        };
        assert_eq!(resume_label(&mins), "Resume S1 E3 (23m left)");
        let movie = WatchProgress::default();
        assert_eq!(resume_label(&movie), "Resume");
    }

    #[test]
    fn episode_mark_partial_watched_and_unknown() {
        assert_eq!(episode_mark(0, 1_000), (0.0, false));
        assert_eq!(episode_mark(500, 0), (0.0, false));
        let (half, watched) = episode_mark(500, 1_000);
        assert!((half - 0.5).abs() < 1e-6 && !watched);
        assert!(episode_mark(800, 1_000).1, "80 % counts as watched");
        assert!((episode_mark(2_000, 1_000).0 - 1.0).abs() < 1e-6);
    }

    #[test]
    fn format_details_meta_extracts_badges_and_facts() {
        let page = DetailsPage {
            title: "Severance".into(),
            plot: String::new(),
            resume: String::new(),
            resume_media_key: None,
            episodes: Vec::new(),
            poster_url: None,
            background_url: None,
            provider: "CineStream".into(),
            url: String::new(),
            data: String::new(),
            media_key: String::new(),
            skip_ids: skip_times::ExternalIds::default(),
            media_type: "TvSeries".into(),
            year: Some(2022),
            rating: Some(6800),
            duration_minutes: Some(52),
            tags: Vec::new(),
            cast_members: Vec::new(),
        };
        let meta = format_details_meta(&page);
        assert_eq!(meta.media_type.as_str(), "TVSERIES");
        assert_eq!(meta.provider.as_str(), "CineStream");
        assert_eq!(meta.year.as_str(), "2022");
        assert_eq!(meta.rating.as_str(), "6.8");
        assert_eq!(meta.duration.as_str(), "52 min");
    }
}
