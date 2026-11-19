//! The seek bar's extras and the skip button on screen: the buffered stretch, chapter dividers,
//! the intro / credits marks, the hover text, "Skip Intro" / "Next episode" and the episode
//! buttons.

use player_api::{Chapter, PlaybackStatus};
use slint::{ModelRc, SharedString, VecModel};

use crate::seek_bar::{buffered_fraction, chapter_notches, hover_text};
use crate::skip_segments::{
    merge_segments, segment_at, segments_from_chapters, skip_prompt, PromptAction, Segment,
    SkipPrompt,
};
use crate::{CloudStream, SeekMark};

use super::bridge_context::BridgeContext;
use super::details_callbacks::play_episode;
use super::playback_reactions::play_next_episode;
use super::player_callbacks::report;
use super::skip_lookup_start::start_online_lookup;

/// What the player knows about the playing stream's parts.
#[derive(Debug, Default)]
pub(crate) struct Timeline {
    pub(crate) chapters: Vec<Chapter>,
    /// The parts the chapters name, and those an online database knows.
    chapter_parts: Vec<Segment>,
    online: Vec<Segment>,
    /// Both merged: what the bar and the skip button use.
    segments: Vec<Segment>,
    /// The stream length the chapters were read for (None: not yet).
    pub(crate) read_for: Option<i64>,
    prompt: Option<SkipPrompt>,
}

pub(crate) fn wire_timeline(ctx: &BridgeContext, ui: &CloudStream) {
    let c = ctx.clone();
    ui.on_describe_seek(move |ms| {
        let session = c.session.borrow();
        let timeline = &session.timeline;
        let part = segment_at(&timeline.segments, ms as i64).map(|s| s.kind.name());
        SharedString::from(hover_text(ms as i64, &timeline.chapters, part))
    });
    let c = ctx.clone();
    ui.on_skip_segment(move || take_prompt(&c));
    let c = ctx.clone();
    ui.on_next_episode(move || play_next_episode(&c));
    let c = ctx.clone();
    ui.on_previous_episode(move || {
        if let Some(index) = previous_episode(&c) {
            play_episode(&c, index);
        }
    });
}

/// True when the stream length changed or the chapter list arrived / changed.
pub(crate) fn should_reread_parts(
    read_for: Option<i64>,
    duration_ms: i64,
    stored: &[Chapter],
    live: &[Chapter],
) -> bool {
    read_for != Some(duration_ms) || stored != live
}

/// Follows the player: how much is loaded, the stream's parts (once per stream length), the
/// skip button and whether there is an episode before / after.
pub(crate) fn show_timeline(ctx: &BridgeContext, ui: &CloudStream, status: &PlaybackStatus) {
    ui.set_buffered_fraction(buffered_fraction(
        status.buffered_ms,
        status.position_ms,
        status.duration_ms,
    ));
    refresh_parts_if_needed(ctx, ui, status.duration_ms);
    let has_next = ctx
        .session
        .borrow()
        .target
        .as_ref()
        .is_some_and(|item| item.next_episode.is_some());
    let prompt = {
        let segments = &ctx.session.borrow().timeline.segments;
        skip_prompt(segments, status.position_ms, status.duration_ms, has_next)
    };
    ui.set_skip_label(prompt.map_or("", |p| p.label).into());
    ui.set_skip_key(prompt_key(prompt).into());
    ui.set_has_next_episode(has_next);
    ui.set_has_previous_episode(previous_episode(ctx).is_some());
    ctx.session.borrow_mut().timeline.prompt = prompt;
}

fn refresh_parts_if_needed(ctx: &BridgeContext, ui: &CloudStream, duration_ms: i64) {
    let Some(playback) = &ctx.services.playback else {
        return;
    };
    let live = playback.chapters();
    let needed = {
        let timeline = &ctx.session.borrow().timeline;
        should_reread_parts(timeline.read_for, duration_ms, &timeline.chapters, &live)
    };
    if needed {
        apply_parts(ctx, ui, duration_ms, live);
    }
}

/// Applies chapter parts; a new duration also clears online times and starts a fresh lookup.
fn apply_parts(ctx: &BridgeContext, ui: &CloudStream, duration_ms: i64, chapters: Vec<Chapter>) {
    let chapter_parts = segments_from_chapters(&chapters, duration_ms);
    ui.set_chapter_notches(ModelRc::new(VecModel::from(chapter_notches(
        &chapters,
        duration_ms,
    ))));
    if !chapter_parts.is_empty() {
        tracing::info!(
            parts = chapter_parts.len(),
            "skip parts found in the chapters"
        );
    }
    let (segments, duration_changed) = store_parts(ctx, duration_ms, chapters, chapter_parts);
    ui.set_skip_marks(ModelRc::new(VecModel::from(marks(&segments, duration_ms))));
    if duration_changed && duration_ms > 0 {
        start_online_lookup(ctx, duration_ms);
    }
}

fn store_parts(
    ctx: &BridgeContext,
    duration_ms: i64,
    chapters: Vec<Chapter>,
    chapter_parts: Vec<Segment>,
) -> (Vec<Segment>, bool) {
    let mut session = ctx.session.borrow_mut();
    let timeline = &mut session.timeline;
    let duration_changed = timeline.read_for != Some(duration_ms);
    timeline.chapters = chapters;
    timeline.chapter_parts.clone_from(&chapter_parts);
    if duration_changed {
        timeline.online.clear();
    }
    timeline.segments = merge_segments(&chapter_parts, &timeline.online);
    timeline.read_for = Some(duration_ms);
    (timeline.segments.clone(), duration_changed)
}

/// Online skip times arrived for the playing stream: they fill the kinds the chapters miss.
pub(crate) fn show_online_parts(ctx: &BridgeContext, ui: &CloudStream, online: Vec<Segment>) {
    tracing::info!(parts = online.len(), "online skip times found");
    let duration_ms = {
        let mut session = ctx.session.borrow_mut();
        let timeline = &mut session.timeline;
        timeline.segments = merge_segments(&timeline.chapter_parts, &online);
        timeline.online = online;
        timeline.read_for.unwrap_or(0)
    };
    let segments = ctx.session.borrow().timeline.segments.clone();
    ui.set_skip_marks(ModelRc::new(VecModel::from(marks(&segments, duration_ms))));
}

/// The coloured stretches on the bar, as shares of its length.
fn marks(segments: &[Segment], duration_ms: i64) -> Vec<SeekMark> {
    if duration_ms <= 0 {
        return Vec::new();
    }
    let share = |ms: i64| (ms.clamp(0, duration_ms) as f64 / duration_ms as f64) as f32;
    segments
        .iter()
        .map(|segment| SeekMark {
            start: share(segment.start_ms),
            width: share(segment.end_ms) - share(segment.start_ms),
            kind: segment.kind.slot(),
        })
        .collect()
}

/// Names the prompt, so a new part (or entering one again) is told apart from the same part
/// going on: the button shows by itself for its first seconds only when this changes.
fn prompt_key(prompt: Option<SkipPrompt>) -> String {
    match prompt.map(|p| p.action) {
        None => String::new(),
        Some(PromptAction::NextEpisode) => "next".to_owned(),
        Some(PromptAction::Seek(to)) => format!("seek:{to}"),
    }
}

fn take_prompt(ctx: &BridgeContext) {
    let prompt = ctx.session.borrow().timeline.prompt;
    match prompt.map(|p| p.action) {
        Some(PromptAction::Seek(to)) => {
            if let Some(playback) = &ctx.services.playback {
                report("skip", playback.seek_to(to));
            }
        }
        Some(PromptAction::NextEpisode) => play_next_episode(ctx),
        None => {}
    }
}

/// The title page's index of the episode before the playing one.
fn previous_episode(ctx: &BridgeContext) -> Option<usize> {
    let session = ctx.session.borrow();
    let key = session.target.as_ref()?.media_key.as_str();
    let playing = session.episodes.iter().position(|row| row.data == key)?;
    playing.checked_sub(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chapter(title: &str, time_ms: i64) -> Chapter {
        Chapter {
            title: title.into(),
            time_ms,
        }
    }

    #[test]
    fn should_reread_parts_when_duration_or_chapters_change() {
        let opening = [chapter("Opening", 0)];
        assert!(should_reread_parts(None, 1_000, &[], &[]));
        assert!(should_reread_parts(Some(500), 1_000, &opening, &opening));
        assert!(should_reread_parts(Some(1_000), 1_000, &[], &opening));
        assert!(!should_reread_parts(Some(1_000), 1_000, &opening, &opening));
    }
}
