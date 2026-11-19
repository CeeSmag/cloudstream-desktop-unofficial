//! Jumps in the video: a click on the bar goes at once, the arrow keys and ±10 s are spaced out
//! ([`crate::seek_scheduler`]) and the bar shows where they go before the player gets there.

use std::time::Instant;

use crate::feedback_text::seek_text;
use crate::seek_bar::format_time;
use crate::CloudStream;

use super::bridge_context::BridgeContext;
use super::player_callbacks::report;
use super::player_feedback::show_feedback;

pub(crate) fn wire_seeking(ctx: &BridgeContext, ui: &CloudStream) {
    let c = ctx.clone();
    ui.on_seek_to(move |ms| jump_to(&c, ms as i64));
    let c = ctx.clone();
    ui.on_seek_by(move |delta| seek_by(&c, delta as i64));
}

/// A click or drag on the bar: goes at once, and any waiting key jump is forgotten.
fn jump_to(ctx: &BridgeContext, target_ms: i64) {
    let Some(playback) = &ctx.services.playback else {
        return;
    };
    ctx.session.borrow_mut().seeks.cancel();
    report("seek", playback.seek_to(target_ms));
    if let Some(ui) = ctx.window() {
        show_feedback(&ui, format!("Jump to {}", format_time(target_ms)), None);
    }
}

/// An arrow key or ±10 s: counted from where the last jump goes, sent now or a moment later.
fn seek_by(ctx: &BridgeContext, delta_ms: i64) {
    let Some(playback) = &ctx.services.playback else {
        return;
    };
    let status = playback.tick();
    let base = ctx
        .session
        .borrow()
        .seeks
        .pending()
        .unwrap_or(status.position_ms);
    let target = (base + delta_ms).clamp(0, status.duration_ms.max(0));
    let send = ctx.session.borrow_mut().seeks.press(target, Instant::now());
    if let Some(target) = send {
        report("seek", playback.seek_to(target));
    }
    if let Some(ui) = ctx.window() {
        show_position(&ui, target);
        show_feedback(&ui, seek_text(delta_ms), None);
    }
}

/// Sends the waiting key jump once its time has come (called by the frame timer).
pub(crate) fn send_due_seek(ctx: &BridgeContext) {
    let due = ctx.session.borrow_mut().seeks.due(Instant::now());
    if let (Some(target), Some(playback)) = (due, &ctx.services.playback) {
        report("seek", playback.seek_to(target));
    }
}

/// Where the bar and the time say the video is: the waiting jump's target while one waits.
pub(crate) fn shown_position(ctx: &BridgeContext, playing_ms: i64) -> i64 {
    ctx.session.borrow().seeks.pending().unwrap_or(playing_ms)
}

fn show_position(ui: &CloudStream, position_ms: i64) {
    ui.set_position_ms(position_ms as f32);
    ui.set_position_text(format_time(position_ms).into());
}
