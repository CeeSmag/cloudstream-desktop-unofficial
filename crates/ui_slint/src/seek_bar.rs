//! What the player's seek bar draws besides the position: the buffered stretch, where chapters
//! start, and the text over the bar where the pointer is.
//!
//! Ported from the old app (`SeekBar.tsx`, `seekBarHover.ts`).

use player_api::Chapter;

/// `m:ss`, or `h:mm:ss` from an hour on; negative times show as 0:00.
pub(crate) fn format_time(ms: i64) -> String {
    let total = ms.max(0) / 1000;
    let (hours, minutes, seconds) = (total / 3600, (total / 60) % 60, total % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

/// How much of the bar is loaded, 0 to 1: at least up to the position.
pub(crate) fn buffered_fraction(buffered_ms: i64, position_ms: i64, duration_ms: i64) -> f32 {
    if duration_ms <= 0 {
        return 0.0;
    }
    let loaded = buffered_ms.max(position_ms) as f64 / duration_ms as f64;
    loaded.clamp(0.0, 1.0) as f32
}

/// Where chapters start along the bar, 0 to 1, to draw dividers; none unless there are two or more.
pub(crate) fn chapter_notches(chapters: &[Chapter], duration_ms: i64) -> Vec<f32> {
    if duration_ms <= 0 || chapters.len() < 2 {
        return Vec::new();
    }
    let mut notches: Vec<f32> = chapters
        .iter()
        .filter(|chapter| chapter.time_ms > 0 && chapter.time_ms < duration_ms)
        .map(|chapter| (chapter.time_ms as f64 / duration_ms as f64) as f32)
        .collect();
    notches.dedup();
    notches
}

/// The chapter playing at `position_ms`: the last one that has started by then.
pub(crate) fn chapter_at(chapters: &[Chapter], position_ms: i64) -> Option<&Chapter> {
    chapters
        .iter()
        .filter(|chapter| chapter.time_ms <= position_ms)
        .max_by_key(|chapter| chapter.time_ms)
}

/// Structured parts for the seek bar tooltip: `(time, chapter, part)`.
pub(crate) fn hover_parts(
    position_ms: i64,
    chapters: &[Chapter],
    part: Option<&str>,
) -> (String, String, String) {
    let time = format_time(position_ms);
    let chapter = chapter_at(chapters, position_ms)
        .map(|chapter| chapter.title.clone())
        .unwrap_or_default();
    let part = part.unwrap_or_default().to_owned();
    (time, chapter, part)
}

/// The text over the bar for the point at `position_ms`: its time, then the chapter and the part
/// of the video (`Intro`) when known.
#[cfg(test)]
pub(crate) fn hover_text(position_ms: i64, chapters: &[Chapter], part: Option<&str>) -> String {
    let (mut text, chapter, part) = hover_parts(position_ms, chapters, part);
    if !chapter.is_empty() {
        text.push_str("  ");
        text.push_str(&chapter);
    }
    if !part.is_empty() {
        text.push_str(" · ");
        text.push_str(&part);
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chapter(title: &str, time_ms: i64) -> Chapter {
        Chapter {
            title: title.to_owned(),
            time_ms,
        }
    }

    #[test]
    fn format_time_short_and_long_videos() {
        assert_eq!(format_time(0), "0:00");
        assert_eq!(format_time(-5), "0:00");
        assert_eq!(format_time(65_400), "1:05");
        assert_eq!(format_time(3_725_000), "1:02:05");
    }

    #[test]
    fn buffered_fraction_is_at_least_the_position_and_capped() {
        assert!((buffered_fraction(30_000, 10_000, 100_000) - 0.3).abs() < 1e-6);
        assert!((buffered_fraction(0, 50_000, 100_000) - 0.5).abs() < 1e-6);
        assert!((buffered_fraction(500_000, 0, 100_000) - 1.0).abs() < 1e-6);
        assert!(buffered_fraction(10, 10, 0).abs() < 1e-6);
    }

    #[test]
    fn chapter_notches_need_two_chapters_and_skip_the_ends() {
        assert!(chapter_notches(&[chapter("Only", 0)], 100_000).is_empty());
        let notches = chapter_notches(
            &[chapter("A", 0), chapter("B", 25_000), chapter("C", 100_000)],
            100_000,
        );
        assert_eq!(notches, [0.25]);
    }

    #[test]
    fn hover_text_names_time_chapter_and_part() {
        let chapters = [chapter("Opening", 0), chapter("Part A", 90_000)];
        assert_eq!(hover_text(30_000, &[], None), "0:30");
        assert_eq!(
            hover_text(30_000, &chapters, Some("Intro")),
            "0:30  Opening · Intro"
        );
        assert_eq!(hover_text(95_000, &chapters, None), "1:35  Part A");
    }
}
