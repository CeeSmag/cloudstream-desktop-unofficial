//! Which title the Home banner offers to continue, and its facts line.
//!
//! Ported from the old app (`resumeHero.ts`): the newest Continue watching entry of the selected
//! provider that has a title page and is not finished (80 % or more watched). A title watched on
//! another provider is never offered, so a blocked or unverified site cannot cause errors while
//! the viewer is on a different one.

use core_model::WatchProgress;

/// From this share of an episode on it counts as watched.
const WATCHED_FRACTION: f64 = 0.8;

/// The place in `list` (newest first) of the title the banner offers; None for no banner.
pub(crate) fn pick_resume_hero(list: &[WatchProgress], provider: &str) -> Option<usize> {
    list.iter().position(|item| {
        item.provider == provider && !item.detail_url.is_empty() && !finished(item)
    })
}

/// Watched to 80 % or more (an unknown length never counts as finished).
fn finished(item: &WatchProgress) -> bool {
    watched_percent(item).is_some_and(|percent| f64::from(percent) >= WATCHED_FRACTION * 100.0)
}

/// How much was watched, 0 to 100; None when the length is unknown or it was not started.
fn watched_percent(item: &WatchProgress) -> Option<u32> {
    if item.duration_ms <= 0 || item.position_ms <= 0 {
        return None;
    }
    let fraction = (item.position_ms as f64 / item.duration_ms as f64).min(1.0);
    Some((fraction * 100.0).round() as u32)
}

/// "S2 E5 · 54% watched · 22m left", or "S2 E6 · Up next" for an episode not started yet.
pub(crate) fn resume_facts(item: &WatchProgress) -> String {
    let mut parts = Vec::new();
    if let (Some(season), Some(episode)) = (item.season, item.episode) {
        parts.push(format!("S{season} E{episode}"));
    }
    match watched_percent(item) {
        Some(percent) => {
            parts.push(format!("{percent}% watched"));
            let left = (item.duration_ms - item.position_ms).max(0) / 60_000;
            parts.push(format!("{left}m left"));
        }
        None => parts.push("Up next".to_owned()),
    }
    parts.join(" · ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(provider: &str, position_ms: i64, duration_ms: i64) -> WatchProgress {
        WatchProgress {
            provider: provider.into(),
            detail_url: format!("https://{provider}/show"),
            position_ms,
            duration_ms,
            season: Some(2),
            episode: Some(5),
            ..WatchProgress::default()
        }
    }

    #[test]
    fn pick_resume_hero_takes_the_newest_unfinished_of_the_provider() {
        let list = [
            item("Other", 100, 1_000),
            item("Mine", 900, 1_000),
            item("Mine", 300, 1_000),
            item("Mine", 100, 1_000),
        ];
        assert_eq!(
            pick_resume_hero(&list, "Mine"),
            Some(2),
            "90 % counts as finished"
        );
        assert_eq!(pick_resume_hero(&list, "Nobody"), None);
        let mut no_page = item("Mine", 100, 1_000);
        no_page.detail_url.clear();
        assert_eq!(pick_resume_hero(&[no_page], "Mine"), None);
    }

    #[test]
    fn pick_resume_hero_offers_an_episode_not_started_yet_or_of_unknown_length() {
        assert_eq!(pick_resume_hero(&[item("Mine", 0, 0)], "Mine"), Some(0));
        assert_eq!(pick_resume_hero(&[item("Mine", 5_000, 0)], "Mine"), Some(0));
    }

    #[test]
    fn resume_facts_partial_and_up_next() {
        assert_eq!(
            resume_facts(&item("P", 1_620_000, 3_000_000)),
            "S2 E5 · 54% watched · 23m left"
        );
        assert_eq!(resume_facts(&item("P", 0, 3_000_000)), "S2 E5 · Up next");
    }
}
