//! Builds a title page from the provider's answer and the viewer's history.

use app_services::UiServices;
use core_model::{CastMember, LoadDetails, WatchProgress};

use crate::ui_events::{DetailsPage, EpisodeRow};

use super::session::DetailsTarget;

/// The page for `details`, with where the viewer left off and which episodes they watched.
pub(crate) fn details_page(
    services: &UiServices,
    target: DetailsTarget,
    details: LoadDetails,
    fallback_title: String,
) -> DetailsPage {
    let library = &services.library;
    let pointer = match library.resume_pointer(&target.provider, &target.url) {
        Ok(pointer) => pointer,
        Err(error) => {
            tracing::warn!(%error, "could not read where the title was left off");
            None
        }
    };
    let progress = library
        .show_progress(&target.provider, &target.url)
        .unwrap_or_default();
    let data = match &details.data_url {
        Some(data_url) if details.episodes.is_empty() => data_url.clone(),
        _ => target.url.clone(),
    };
    DetailsPage {
        title: if details.name.is_empty() {
            fallback_title
        } else {
            details.name.clone()
        },
        plot: details.plot.clone().unwrap_or_default(),
        meta: details_meta(&details),
        resume: pointer.as_ref().map(resume_label).unwrap_or_default(),
        resume_media_key: pointer.map(|pointer| pointer.media_key),
        episodes: episode_rows(&details, &progress),
        poster_url: details.poster_url.clone(),
        provider: target.provider,
        url: target.url,
        data,
        media_key: target.media_key,
        tags: details.tags.clone().unwrap_or_default(),
        cast: details.actors.iter().map(cast_text).collect(),
    }
}

/// "Name (role)", or the name alone when the provider gives no role.
pub(crate) fn cast_text(member: &CastMember) -> String {
    match member.role.as_deref().map(str::trim) {
        Some(role) if !role.is_empty() => format!("{} ({role})", member.name),
        _ => member.name.clone(),
    }
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

/// "Resume S1 E3 · 23 min left" (the time only when the length is known).
pub(crate) fn resume_label(pointer: &WatchProgress) -> String {
    let place = match (pointer.season, pointer.episode) {
        (Some(season), Some(episode)) => format!("Resume S{season} E{episode}"),
        _ => "Resume".to_owned(),
    };
    let left_minutes = (pointer.duration_ms - pointer.position_ms) / 60_000;
    if pointer.duration_ms > 0 && left_minutes > 0 {
        format!("{place} · {left_minutes} min left")
    } else {
        place
    }
}

/// "2024 · 2h 15m · ★ 7.8" from what the provider knows.
pub(crate) fn details_meta(details: &LoadDetails) -> String {
    let mut parts = Vec::new();
    if let Some(year) = details.year {
        parts.push(year.to_string());
    }
    if let Some(minutes) = details.duration.filter(|minutes| *minutes > 0) {
        parts.push(match (minutes / 60, minutes % 60) {
            (0, rest) => format!("{rest}m"),
            (hours, 0) => format!("{hours}h"),
            (hours, rest) => format!("{hours}h {rest}m"),
        });
    }
    if let Some(rating) = details.rating.filter(|rating| *rating > 0) {
        // Providers give the rating out of 10 000 (CloudStream's scale); shown out of 10.
        parts.push(format!("★ {:.1}", f64::from(rating) / 1000.0));
    }
    if !details.episodes.is_empty() {
        parts.push(format!("{} episodes", details.episodes.len()));
    }
    parts.join("  ·  ")
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
            let (fraction, watched) = progress
                .iter()
                .find(|seen| seen.media_key == episode.data)
                .map_or((0.0, false), |seen| {
                    episode_mark(seen.position_ms, seen.duration_ms)
                });
            EpisodeRow {
                name: episode.name.clone().unwrap_or_else(|| code.clone()),
                watched,
                progress: fraction,
                description: episode.description.clone().unwrap_or_default(),
                code,
                data: episode.data.clone(),
                season: episode.season.unwrap_or(-1),
                episode: episode.episode,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn details(year: Option<i32>, duration: Option<i32>, rating: Option<i32>) -> LoadDetails {
        LoadDetails {
            year,
            duration,
            rating,
            ..LoadDetails::default()
        }
    }

    #[test]
    fn resume_label_shows_place_and_time_left() {
        let pointer = WatchProgress {
            season: Some(1),
            episode: Some(3),
            position_ms: 10 * 60_000,
            duration_ms: 33 * 60_000,
            ..WatchProgress::default()
        };
        assert_eq!(resume_label(&pointer), "Resume S1 E3 · 23 min left");
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
    fn cast_text_adds_the_role_when_known() {
        let member = |role: Option<&str>| CastMember {
            name: "Bryan Cranston".into(),
            role: role.map(str::to_owned),
            ..CastMember::default()
        };
        assert_eq!(cast_text(&member(Some("Main"))), "Bryan Cranston (Main)");
        assert_eq!(cast_text(&member(Some(" "))), "Bryan Cranston");
        assert_eq!(cast_text(&member(None)), "Bryan Cranston");
    }

    #[test]
    fn details_meta_joins_what_is_known() {
        assert_eq!(
            details_meta(&details(Some(2024), Some(135), Some(7800))),
            "2024  ·  2h 15m  ·  ★ 7.8"
        );
        assert_eq!(details_meta(&details(None, Some(45), None)), "45m");
        assert_eq!(details_meta(&details(None, Some(120), None)), "2h");
        assert_eq!(details_meta(&details(None, None, None)), "");
    }
}
