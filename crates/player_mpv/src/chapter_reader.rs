use player_api::Chapter;

/// A chapter time as mpv reports it (seconds, such as `123.456`) in milliseconds; `None` for text
/// that is not a usable time.
fn time_ms(text: &str) -> Option<i64> {
    let seconds: f64 = text.trim().parse().ok()?;
    if !seconds.is_finite() || seconds < 0.0 {
        return None;
    }
    Some((seconds * 1000.0).round() as i64)
}

/// Builds the chapter list from mpv's `chapter-list/N/...` properties, in time order. A chapter
/// without a readable time is left out; one without a name is called `Chapter N`. `get` returns a
/// property as text.
pub(crate) fn read_chapters(get: impl Fn(&str) -> Option<String>) -> Vec<Chapter> {
    let count: usize = get("chapter-list/count")
        .and_then(|text| text.parse().ok())
        .unwrap_or(0);
    let mut chapters: Vec<Chapter> = (0..count)
        .filter_map(|index| {
            let time_ms = time_ms(&get(&format!("chapter-list/{index}/time"))?)?;
            let title = get(&format!("chapter-list/{index}/title"))
                .map(|text| text.trim().to_owned())
                .filter(|text| !text.is_empty())
                .unwrap_or_else(|| format!("Chapter {}", index + 1));
            Some(Chapter { title, time_ms })
        })
        .collect();
    chapters.sort_by_key(|chapter| chapter.time_ms);
    chapters
}

#[cfg(test)]
#[path = "chapter_live_tests.rs"]
mod live_tests;
#[cfg(test)]
#[path = "chapter_reader_tests.rs"]
mod tests;
